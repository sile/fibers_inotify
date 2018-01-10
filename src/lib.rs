//! A [futures] friendly [inotify] wrapper for [fibers] crate.
//!
//! [futures]: https://crates.io/crates/futures
//! [fibers]: https://crates.io/crates/fibers
//! [inotify]: https://en.wikipedia.org/wiki/Inotify
//!
//! # Examples
//!
//! Watches `/tmp` directory:
//!
//! ```
//! # extern crate fibers;
//! # extern crate fibers_inotify;
//! # extern crate futures;
//! use fibers::{Executor, InPlaceExecutor, Spawn};
//! use fibers_inotify::{InotifyService, WatchMask};
//! use futures::{Future, Stream};
//!
//! # fn main() {
//! let inotify_service = InotifyService::new();
//! let inotify_handle = inotify_service.handle();
//!
//! let mut executor = InPlaceExecutor::new().unwrap();
//! executor.spawn(inotify_service.map_err(|e| panic!("{}", e)));
//!
//! executor.spawn(
//!    inotify_handle
//!        .watch("/tmp/", WatchMask::CREATE | WatchMask::DELETE)
//!        .for_each(|event| Ok(println!("# EVENT: {:?}", event)))
//!        .map_err(|e| panic!("{}", e)),
//!    );
//!
//! executor.run_once().unwrap();
//! # }
//! ```
#![warn(missing_docs)]
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
pub use service::{InotifyService, InotifyServiceHandle};
pub use watcher::{Watcher, WatcherEvent};

mod error;
mod internal_inotify;
mod mio_ext;
mod service;
mod watcher;

/// This crate specific `Result` type.
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
                .watch("/tmp/", WatchMask::all())
                .for_each(|_event| Ok(()))
                .map_err(|e| panic!("{}", e)),
        );
        executor.spawn(
            inotify
                .watch("/tmp/", WatchMask::CREATE)
                .for_each(|_event| Ok(()))
                .map_err(|e| panic!("{}", e)),
        );

        for _ in 0..500 {
            executor.run_once().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
    }
}
