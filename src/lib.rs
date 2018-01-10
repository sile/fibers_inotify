extern crate fibers;
extern crate futures;
extern crate inotify;
extern crate inotify_sys;
extern crate libc;
extern crate mio;
#[macro_use]
extern crate trackable;

#[doc(no_inline)]
pub use inotify::{EventMask, WatchMask};

pub use error::{Error, ErrorKind};
pub use internal_inotify::InotifyEvent;
pub use service::{InotifyService, InotifyServiceHandle, Watcher, WatcherEvent};

mod error;
mod internal_inotify;
mod mio_ext;
mod service;

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test {
    use std::thread;
    use std::time::Duration;
    use fibers::{Executor, InPlaceExecutor, Spawn};
    use futures::{Future, Stream};
    use super::*;

    #[test]
    fn it_works() {
        let service = InotifyService::new();
        let inotify = service.handle();

        let mut executor = InPlaceExecutor::new().unwrap();
        executor.spawn(service.map_err(|e| panic!("{}", e)));

        executor.spawn(
            inotify
                .watcher("/tmp/", WatchMask::all())
                .for_each(|_event| Ok(()))
                .map_err(|e| panic!("{}", e)),
        );
        executor.spawn(
            inotify
                .watcher("/tmp/", WatchMask::CREATE)
                .for_each(|_event| Ok(()))
                .map_err(|e| panic!("{}", e)),
        );

        for _ in 0..500 {
            executor.run_once().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
    }
}
