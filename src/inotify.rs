use std::collections::VecDeque;
use std::ffi::{CStr, CString, OsString};
use std::fs::File;
use std::io::{self, Read};
use std::marker::PhantomData;
use std::mem;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::path::{Path, PathBuf};
use fibers;
use fibers::io::poll::{EventedHandle, Interest, Register};
use fibers::sync::oneshot::Monitor;
use futures::{Async, Future, Poll, Stream};
use inotify_sys;
use libc;

use {Error, ErrorKind, EventMask, Result, WatchMask};
use mio_ext::OwnedEventedFd;

#[derive(Debug)]
pub struct Inotify {
    file: File,
    events: VecDeque<Event>,
    read_monitor: ReadMonitor,
    _cannot_sync: PhantomData<*const ()>,
}
unsafe impl Send for Inotify {}
impl Inotify {
    pub fn new() -> Result<Self> {
        let flags = inotify_sys::IN_NONBLOCK;
        let fd = unsafe { inotify_sys::inotify_init1(flags) };
        if fd == -1 {
            Err(track!(Error::last_os_error()))
        } else {
            Ok(Inotify {
                file: unsafe { File::from_raw_fd(fd) },
                events: VecDeque::new(),
                read_monitor: track!(ReadMonitor::new(fd))?,
                _cannot_sync: PhantomData,
            })
        }
    }
    pub fn add_watch<P: AsRef<Path>>(
        &mut self,
        path: P,
        mask: WatchMask,
    ) -> Result<WatchDecriptor> {
        let wd = unsafe {
            let path = track!(
                CString::new(path.as_ref().to_path_buf().into_os_string().into_vec())
                    .map_err(Error::from)
            )?;
            inotify_sys::inotify_add_watch(self.file.as_raw_fd(), path.as_ptr(), mask.bits())
        };
        if wd == -1 {
            Err(track!(Error::last_os_error()))
        } else {
            Ok(WatchDecriptor(wd))
        }
    }
    pub fn remove_watch(&mut self, wd: WatchDecriptor) -> Result<()> {
        let result = unsafe { inotify_sys::inotify_rm_watch(self.file.as_raw_fd(), wd.0) };
        if result == -1 {
            Err(track!(Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    fn read_event(&mut self) -> Result<Option<Event>> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }

        let mut buf = [0; 4096];
        match self.file.read(&mut buf) {
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    Ok(None)
                } else {
                    Err(track!(Error::from(e)))
                }
            }
            Ok(read_size) => {
                let mut offset = 0;
                while offset < read_size {
                    let raw_event: &inotify_sys::inotify_event =
                        unsafe { mem::transmute((&buf[offset..]).as_ptr()) };
                    offset += mem::size_of::<inotify_sys::inotify_event>() + raw_event.len as usize;
                    track_assert!(offset <= read_size, ErrorKind::Other);

                    let name = if raw_event.len == 0 {
                        None
                    } else {
                        let start = offset - raw_event.len as usize;
                        let mut end = offset;
                        while end > 1 && buf[end - 1] == b'\0' && buf[end - 2] == b'\0' {
                            end -= 1;
                        }
                        let name = track!(
                            CStr::from_bytes_with_nul(&buf[start..end]).map_err(Error::from)
                        )?;
                        let name = PathBuf::from(OsString::from_vec(name.to_bytes().to_owned()));
                        Some(name)
                    };
                    let event = Event {
                        wd: WatchDecriptor(raw_event.wd),
                        mask: EventMask::from_bits_truncate(raw_event.mask),
                        cookie: raw_event.cookie,
                        name,
                    };
                    self.events.push_back(event);
                }
                Ok(self.events.pop_front())
            }
        }
    }
}
impl Stream for Inotify {
    type Item = Event;
    type Error = Error;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        track!(self.read_monitor.poll())?;
        if let Some(event) = track!(self.read_event())? {
            Ok(Async::Ready(Some(event)))
        } else {
            Ok(Async::NotReady)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WatchDecriptor(pub(crate) libc::c_int);

#[derive(Debug, Clone)]
pub struct Event {
    pub wd: WatchDecriptor,
    pub mask: EventMask,
    pub cookie: u32,
    pub name: Option<PathBuf>,
}

#[derive(Debug)]
struct ReadMonitor {
    register: Register<OwnedEventedFd>,
    handle: Option<EventedHandle<OwnedEventedFd>>,
    monitor: Option<Monitor<(), io::Error>>,
}
impl ReadMonitor {
    fn new(fd: RawFd) -> Result<Self> {
        let register = fibers::fiber::with_current_context(|mut context| {
            context.poller().register(OwnedEventedFd(fd))
        });

        Ok(ReadMonitor {
            register: track_assert_some!(register, ErrorKind::Other, "Not in a fiber context"),
            handle: None,
            monitor: None,
        })
    }
}
impl Future for ReadMonitor {
    type Item = ();
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            if let Some(handle) = self.handle.as_ref() {
                if track!(self.monitor.poll().map_err(Error::from))?.is_ready() {
                    self.monitor = Some(handle.monitor(Interest::Read));
                    continue;
                }
                return Ok(Async::NotReady);
            }
            if let Async::Ready(handle) = track!(self.register.poll().map_err(Error::from))? {
                self.handle = Some(handle);
                continue;
            }
            return Ok(Async::NotReady);
        }
    }
}
