use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

use failure::Error;

use libc;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr};
use nix::sys::termios::{SetArg, Termios};
use nix::unistd;

use futures::prelude::*;
use tokio::prelude::*;
use tokio::reactor::PollEvented2;

use crate::evented_file::EventedFile;

fn dup_nonblock(fd: RawFd) -> Result<RawFd, failure::Error> {
    // Note: Since `dup` returns `RawFd`, we need to manually `close` it
    // on errors.
    let fd = unistd::dup(fd)?;

    let mut flags = match fcntl(fd, FcntlArg::F_GETFL) {
        Ok(flags) => OFlag::from_bits_truncate(flags),
        Err(e) => {
            let _ = unistd::close(fd);
            return Err(e.into());
        }
    };

    flags.insert(OFlag::O_NONBLOCK);

    match fcntl(fd, FcntlArg::F_SETFL(flags)) {
        Ok(_) => Ok(fd),
        Err(e) => {
            let _ = unistd::close(fd);
            Err(e.into())
        }
    }
}

pub struct RawTermRead {
    stdin: PollEvented2<EventedFile>,
}

pub struct RawTermWrite {
    prev_attrs: Termios,
    stdout: PollEvented2<EventedFile>,
}

pub fn pair() -> Result<(RawTermRead, RawTermWrite), Error> {
    Ok((RawTermRead::new()?, RawTermWrite::new()?))
}

impl RawTermRead {
    pub fn new() -> Result<Self, Error> {
        let stdin = PollEvented2::new({
            let fd = dup_nonblock(libc::STDIN_FILENO)?;
            unsafe { EventedFile::from_raw_fd(fd) }
        });

        Ok(RawTermRead {
            stdin: stdin,
        })
    }
}

impl Read for RawTermRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stdin.read(buf)
    }
}

impl AsyncRead for RawTermRead {}

impl RawTermWrite {
    pub fn new() -> Result<Self, Error> {
        let stdout = PollEvented2::new({
            let fd = dup_nonblock(libc::STDOUT_FILENO)?;
            unsafe { EventedFile::from_raw_fd(fd) }
        });

        let attrs = tcgetattr(stdout.get_ref().as_raw_fd())?;

        let mut raw_attrs = attrs.clone();
        cfmakeraw(&mut raw_attrs);

        tcsetattr(stdout.get_ref().as_raw_fd(), SetArg::TCSANOW, &raw_attrs)?;

        Ok(RawTermWrite {
            prev_attrs: attrs,
            stdout: stdout,
        })
    }
}

impl Drop for RawTermWrite {
    fn drop(&mut self) {
        let _ = tcsetattr(
            self.stdout.get_ref().as_raw_fd(),
            SetArg::TCSANOW,
            &self.prev_attrs,
        );
    }
}

impl Write for RawTermWrite {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdout.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }
}

impl AsyncWrite for RawTermWrite {
    fn shutdown(&mut self) -> io::Result<Async<()>> {
        self.stdout.shutdown()
    }
}
