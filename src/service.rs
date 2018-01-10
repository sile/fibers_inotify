use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use fibers::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use {Error, ErrorKind, Result, WatchMask, Watcher, WatcherEvent};
use internal_inotify::{Inotify, WatchDecriptor};
use watcher::WatcherId;

/// [Inotify] service.
///
/// This is a [`Future`] that never terminate except error cases.
/// Internally it manages zero or more file descriptors of [inotify] as needed.
///
/// [inotify]: https://en.wikipedia.org/wiki/Inotify
/// [`Future`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html
#[derive(Debug)]
pub struct InotifyService {
    inotifies: Vec<InotifyState>,
    command_tx: mpsc::Sender<Command>,
    command_rx: mpsc::Receiver<Command>,
    watcher_id: Arc<AtomicUsize>,
    watchers: HashMap<WatcherId, WatcherState>,
}
impl InotifyService {
    /// Makes a new `InotifyService` instance.
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        InotifyService {
            inotifies: Vec::new(),
            command_tx,
            command_rx,
            watcher_id: Arc::new(AtomicUsize::new(0)),
            watchers: HashMap::new(),
        }
    }

    /// Returns the handle of this service.
    pub fn handle(&self) -> InotifyServiceHandle {
        InotifyServiceHandle {
            command_tx: self.command_tx.clone(),
            watcher_id: Arc::clone(&self.watcher_id),
        }
    }

    fn handle_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::RegisterWatcher {
                watcher_id,
                path,
                mask,
                event_tx,
            } => {
                let watcher = WatcherState {
                    id: watcher_id,
                    inotify_index: 0,
                    wd: WatchDecriptor(-1), // dummy (updated in `register_watcher()`)
                    path,
                    mask,
                    event_tx,
                };
                track!(self.register_watcher(watcher))?;
            }
            Command::DeregisterWatcher { watcher_id } => {
                track!(self.deregister_watcher(watcher_id))?;
            }
        }
        Ok(())
    }
    fn register_watcher(&mut self, mut watcher: WatcherState) -> Result<()> {
        track_assert!(!self.watchers.contains_key(&watcher.id), ErrorKind::Other);
        let is_succeeded = track!(self.add_watch(&mut watcher))?;
        if is_succeeded {
            self.watchers.insert(watcher.id, watcher);
        }
        Ok(())
    }
    fn deregister_watcher(&mut self, watcher_id: WatcherId) -> Result<()> {
        if let Some(watcher) = self.watchers.remove(&watcher_id) {
            let mut i = watcher.inotify_index;
            track!(self.inotifies[i].inotify.remove_watch(watcher.wd))?;
            track_assert_some!(self.inotifies[i].wds.remove(&watcher.wd), ErrorKind::Other);
            while i + 1 == self.inotifies.len() && self.inotifies[i].wds.is_empty() {
                self.inotifies.pop();
                i -= 1;
            }
        }
        Ok(())
    }
    fn add_watch(&mut self, watcher: &mut WatcherState) -> Result<bool> {
        let i = watcher.inotify_index;
        if i == self.inotifies.len() {
            self.inotifies.push(track!(InotifyState::new())?);
        }

        let mut mask = watcher.mask;
        mask.remove(WatchMask::MASK_ADD);
        let result = track!(self.inotifies[i].inotify.add_watch(&watcher.path, mask));
        let wd = match result {
            Err(e) => {
                let _ = watcher.event_tx.send(Err(e));
                return Ok(false);
            }
            Ok(wd) => wd,
        };

        if let Some(overwritten_id) = self.inotifies[i].wds.insert(wd, watcher.id) {
            let mut overwritten_watcher =
                track_assert_some!(self.watchers.remove(&overwritten_id), ErrorKind::Other);
            overwritten_watcher.inotify_index = i + 1;
            track!(self.add_watch(&mut overwritten_watcher))?;
            self.watchers
                .insert(overwritten_watcher.id, overwritten_watcher);
        }

        watcher.wd = wd;
        let event = if i == 0 {
            WatcherEvent::StartWatching
        } else {
            WatcherEvent::RestartWatching
        };
        let _ = watcher.event_tx.send(Ok(event));
        Ok(true)
    }
}
impl Future for InotifyService {
    type Item = ();
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Async::Ready(Some(command)) = self.command_rx.poll().expect("Never fails") {
            track!(self.handle_command(command))?;
        }
        for inotify in &mut self.inotifies {
            while let Async::Ready(Some(event)) = track!(inotify.inotify.poll())? {
                let watcher_id = inotify.wds[&event.wd];
                let _ = self.watchers[&watcher_id]
                    .event_tx
                    .send(Ok(WatcherEvent::Notified(event)));
            }
        }
        Ok(Async::NotReady)
    }
}
impl Default for InotifyService {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle of `InotifyService`.
#[derive(Debug, Clone)]
pub struct InotifyServiceHandle {
    command_tx: mpsc::Sender<Command>,
    watcher_id: Arc<AtomicUsize>,
}
impl InotifyServiceHandle {
    /// Makes a new `Watcher` that watches `path` with the given mask.
    ///
    /// If the inode indicated by the path has already been watched by other watchers,
    /// the one of existing watcher will be got kicked out and the new one will be added instead.
    /// After that the service will create new inotify instance (i.e., file descriptor) and
    /// re-add the victim watcher to it.
    /// In that case the re-added watcher will receive the event `WatcherEvent::RestartWatching`.
    pub fn watch<P: AsRef<Path>>(&self, path: P, mask: WatchMask) -> Watcher {
        let watcher_id = self.watcher_id.fetch_add(1, Ordering::SeqCst);
        let (event_tx, event_rx) = mpsc::channel();
        let command = Command::RegisterWatcher {
            watcher_id,
            path: path.as_ref().to_path_buf(),
            mask,
            event_tx,
        };
        let _ = self.command_tx.send(command);
        Watcher::new(watcher_id, self.clone(), event_rx)
    }

    pub(crate) fn deregister_watcher(&self, watcher_id: WatcherId) {
        let command = Command::DeregisterWatcher { watcher_id };
        let _ = self.command_tx.send(command);
    }
}

#[derive(Debug)]
enum Command {
    RegisterWatcher {
        watcher_id: WatcherId,
        path: PathBuf,
        mask: WatchMask,
        event_tx: mpsc::Sender<Result<WatcherEvent>>,
    },
    DeregisterWatcher {
        watcher_id: WatcherId,
    },
}

#[derive(Debug)]
struct WatcherState {
    id: WatcherId,
    inotify_index: usize,
    wd: WatchDecriptor,
    path: PathBuf,
    mask: WatchMask,
    event_tx: mpsc::Sender<Result<WatcherEvent>>,
}

#[derive(Debug)]
struct InotifyState {
    inotify: Inotify,
    wds: HashMap<WatchDecriptor, WatcherId>,
}
impl InotifyState {
    fn new() -> Result<Self> {
        Ok(InotifyState {
            inotify: track!(Inotify::new())?,
            wds: HashMap::new(),
        })
    }
}
