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
