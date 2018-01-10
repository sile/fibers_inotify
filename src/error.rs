use std::ffi;
use std::io;
use std::sync::mpsc::RecvError;
use fibers::sync::oneshot::MonitorError;
use libc;
use trackable::error::TrackableError;
use trackable::error::{ErrorKind as TrackableErrorKind, ErrorKindExt};

#[derive(Debug, Clone)]
pub struct Error(TrackableError<ErrorKind>);
derive_traits_for_trackable_error_newtype!(Error, ErrorKind);
impl Error {
    pub fn last_os_error() -> Self {
        Error::from(io::Error::last_os_error())
    }
}
impl From<ffi::NulError> for Error {
    fn from(f: ffi::NulError) -> Self {
        ErrorKind::InvalidInput.cause(f).into()
    }
}
impl From<ffi::FromBytesWithNulError> for Error {
    fn from(f: ffi::FromBytesWithNulError) -> Self {
        ErrorKind::InvalidInput.cause(f).into()
    }
}
impl From<RecvError> for Error {
    fn from(f: RecvError) -> Self {
        ErrorKind::Other.cause(f).into()
    }
}
impl From<io::Error> for Error {
    fn from(f: io::Error) -> Self {
        let kind = f.raw_os_error()
            .map_or(ErrorKind::Other, |errno| match errno {
                libc::EINVAL
                | libc::EACCES
                | libc::EBADF
                | libc::EFAULT
                | libc::ENAMETOOLONG
                | libc::ENOENT => ErrorKind::InvalidInput,
                libc::EMFILE | libc::ENOMEM | libc::ENOSPC => ErrorKind::ResourceShortage,
                _ => ErrorKind::Other,
            });
        kind.cause(f).into()
    }
}
impl<E: Into<Error>> From<MonitorError<E>> for Error {
    fn from(f: MonitorError<E>) -> Self {
        f.map(|e| e.into()).unwrap_or_else(|| {
            ErrorKind::Other
                .cause("monitoring channel disconnected")
                .into()
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidInput,
    ResourceShortage,
    Other,
}
impl TrackableErrorKind for ErrorKind {}
