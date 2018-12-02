use std::io;
use std::fs::File;
use std::os::unix::io::{RawFd, FromRawFd, IntoRawFd, AsRawFd};

use mio::{Evented, Poll, Token, Ready, PollOpt};
use mio::unix::EventedFd;

pub struct EventedIo<T> {
    inner: T,
}

impl<T> EventedIo<T>
where
    T: io::Read + io::Write + AsRawFd,
{
    pub fn new(inner: T) -> Self {
        EventedIo {
            inner: inner,
        }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> Evented for EventedIo<T>
where
    T: AsRawFd,
{
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
        -> io::Result<()>
    {
        EventedFd(&self.inner.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
        -> io::Result<()>
    {
        EventedFd(&self.inner.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.inner.as_raw_fd()).deregister(poll)
    }
}

impl<T> io::Read for EventedIo<T>
where
    T: io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<T> io::Write for EventedIo<T>
where
    T: io::Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<T> IntoRawFd for EventedIo<T>
where
    T: io::Read + io::Write + AsRawFd + IntoRawFd,
{
    fn into_raw_fd(self) -> RawFd {
        self.into_inner().into_raw_fd()
    }
}

impl<T> AsRawFd for EventedIo<T>
where
    T: io::Read + io::Write + AsRawFd + IntoRawFd,
{
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}


pub type EventedFile = EventedIo<File>;

impl FromRawFd for EventedFile {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        EventedFile {
            inner: File::from_raw_fd(fd),
        }
    }
}
