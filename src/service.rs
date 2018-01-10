use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use fibers::sync::mpsc;
use futures::{Async, Future, Poll, Stream};

use {Error, ErrorKind, Result, WatchMask};
use internal_inotify::{Inotify, InotifyEvent, WatchDecriptor};

type WatcherId = usize;

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

#[derive(Debug)]
pub struct InotifyService {
    inotifies: Vec<InotifyState>,
    command_tx: mpsc::Sender<Command>,
    command_rx: mpsc::Receiver<Command>,
    watcher_id: Arc<AtomicUsize>,
    watchers: HashMap<WatcherId, WatcherState>,
}
impl InotifyService {
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
                    wd: WatchDecriptor(-1), // dummy
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
        }

        watcher.wd = wd;
        if i == 0 {
            let _ = watcher.event_tx.send(Ok(WatcherEvent::Started));
        } else {
            let _ = watcher.event_tx.send(Ok(WatcherEvent::Restarted));
        }
        Ok(true)
    }
    fn register_watcher(&mut self, mut watcher: WatcherState) -> Result<()> {
        track_assert!(!self.watchers.contains_key(&watcher.id), ErrorKind::Other);
        if track!(self.add_watch(&mut watcher))? {
            self.watchers.insert(watcher.id, watcher);
        }
        Ok(())
    }
    fn deregister_watcher(&mut self, watcher_id: WatcherId) -> Result<()> {
        let watcher = track_assert_some!(self.watchers.remove(&watcher_id), ErrorKind::Other);
        let i = watcher.inotify_index;
        track!(self.inotifies[i].inotify.remove_watch(watcher.wd))?;
        track_assert_some!(self.inotifies[i].wds.remove(&watcher.wd), ErrorKind::Other);
        Ok(())
    }
}
impl Future for InotifyService {
    type Item = ();
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Async::Ready(Some(command)) = self.command_rx.poll().expect("Never fails") {
            track!(self.handle_command(command))?;
        }
        for inotify in self.inotifies.iter_mut() {
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

#[derive(Debug, Clone)]
pub struct InotifyServiceHandle {
    command_tx: mpsc::Sender<Command>,
    watcher_id: Arc<AtomicUsize>,
}
impl InotifyServiceHandle {
    pub fn watcher<P: AsRef<Path>>(&self, path: P, mask: WatchMask) -> Watcher {
        let watcher_id = self.watcher_id.fetch_add(1, Ordering::SeqCst);
        let (event_tx, event_rx) = mpsc::channel();
        let command = Command::RegisterWatcher {
            watcher_id,
            path: path.as_ref().to_path_buf(),
            mask,
            event_tx,
        };
        let _ = self.command_tx.send(command);
        Watcher {
            id: watcher_id,
            service: self.clone(),
            event_rx,
        }
    }
    fn deregister_watcher(&self, watcher: &Watcher) {
        let command = Command::DeregisterWatcher {
            watcher_id: watcher.id,
        };
        let _ = self.command_tx.send(command);
    }
}

#[derive(Debug)]
pub enum WatcherEvent {
    Started,
    Restarted, // TODO: rename
    Notified(InotifyEvent),
}

#[derive(Debug)]
pub struct Watcher {
    id: WatcherId,
    service: InotifyServiceHandle,
    event_rx: mpsc::Receiver<Result<WatcherEvent>>,
}
impl Stream for Watcher {
    type Item = WatcherEvent;
    type Error = Error;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.event_rx.poll().expect("Never fails") {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(None) => Ok(Async::Ready(None)),
            Async::Ready(Some(result)) => Ok(Async::Ready(Some(track!(result)?))),
        }
    }
}
impl Drop for Watcher {
    fn drop(&mut self) {
        self.service.deregister_watcher(self);
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
