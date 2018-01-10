extern crate fibers;
extern crate futures;
extern crate inotify_sys;
extern crate libc;
extern crate mio;
#[macro_use]
extern crate trackable;

pub use error::{Error, ErrorKind};
pub use mask::{EventMask, WatchMask};

mod error;
pub mod inotify; // TODO: private
mod mask;
mod mio_ext;
pub mod service; // TODO: private

pub type Result<T> = std::result::Result<T, Error>;
