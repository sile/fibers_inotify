use fibers::sync::mpsc;
use futures::{Async, Poll, Stream};

use {Error, EventMask, InotifyEvent, InotifyServiceHandle, Result};

pub type WatcherId = usize;

/// [Inotify] event watcher.
///
/// This is a [`Stream`] that produces a sequence of `WatcherEvent`.
///
/// This stream will terminate if any of the following conditions are satisfied:
///
/// - The associated `InotifyServer` instance is dropped.
/// - The watcher receives an inotify event which has the mask `EventMask::IGNORED`.
///
/// To stop watching, you can drop the `Watcher` instance.
///
/// [inotify]: https://en.wikipedia.org/wiki/Inotify
/// [`Stream`]: https://docs.rs/futures/0.1/futures/stream/trait.Stream.html
#[derive(Debug)]
pub struct Watcher {
    id: WatcherId,
    service: InotifyServiceHandle,
    event_rx: mpsc::Receiver<Result<WatcherEvent>>,
    eos: bool,
}
impl Watcher {
    pub(crate) fn new(
        id: WatcherId,
        service: InotifyServiceHandle,
        event_rx: mpsc::Receiver<Result<WatcherEvent>>,
    ) -> Self {
        Watcher {
            id,
            service,
            event_rx,
            eos: false,
        }
    }
}
impl Stream for Watcher {
    type Item = WatcherEvent;
    type Error = Error;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.eos {
            return Ok(Async::Ready(None));
        }
        match self.event_rx.poll().expect("Never fails") {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(None) => Ok(Async::Ready(None)),
            Async::Ready(Some(result)) => {
                let event = track!(result)?;
                if let WatcherEvent::Notified(ref e) = event {
                    self.eos = e.mask.contains(EventMask::IGNORED);
                }
                Ok(Async::Ready(Some(event)))
            }
        }
    }
}
impl Drop for Watcher {
    fn drop(&mut self) {
        self.service.deregister_watcher(self.id);
    }
}

/// Event produced by `Watcher`.
#[derive(Debug, Clone)]
pub enum WatcherEvent {
    /// The watcher starts watching.
    ///
    /// It means that the specified path and mask are added to an inotify instance successfully.
    StartWatching,

    /// The watcher restarts watching.
    ///
    /// If the inode being watched by this watcher conflicts with a newer watcher's one
    /// in an inotify instance, it will be kicked out and re-added to another inotify instance.
    /// If it happens, this event will be produced.
    /// Note that in such case some inotify events may be lost and the watcher may start watching an inode different from before (althought the path is the same).
    RestartWatching,

    /// Inotify event.
    Notified(InotifyEvent),
}
