use fibers::sync::mpsc;
use futures::{Async, Poll, Stream};

use {Error, InotifyEvent, InotifyServiceHandle, Result};

pub type WatcherId = usize;

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
        }
    }
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
        self.service.deregister_watcher(self.id);
    }
}
