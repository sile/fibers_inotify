use std::io::Result;
use std::os::unix::io::RawFd;
use mio::{Evented, Poll, PollOpt, Ready, Token};
use mio::unix::EventedFd;

#[derive(Debug)]
pub struct OwnedEventedFd(pub RawFd);
impl Evented for OwnedEventedFd {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        EventedFd(&self.0).register(poll, token, interest, opts)
    }
    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        EventedFd(&self.0).reregister(poll, token, interest, opts)
    }
    fn deregister(&self, poll: &Poll) -> Result<()> {
        EventedFd(&self.0).deregister(poll)
    }
}
