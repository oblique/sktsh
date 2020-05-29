use futures::prelude::*;
use libc::{
    cfmakeraw, dup, ioctl, tcgetattr, tcsetattr, termios, winsize,
    STDIN_FILENO, STDOUT_FILENO, TCSANOW, TIOCGWINSZ,
};
use smol::Async;
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::msgs::TermDimensions;

pub struct RawTerm {
    stdin: Async<File>,
    stdout: Async<File>,
    saved_attrs: termios,
}

impl RawTerm {
    pub fn new() -> io::Result<Self> {
        let stdin = unsafe {
            let fd = dup(STDIN_FILENO);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            Async::new(File::from_raw_fd(fd as RawFd))?
        };

        let stdout = unsafe {
            let fd = dup(STDOUT_FILENO);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            Async::new(File::from_raw_fd(fd as RawFd))?
        };

        let saved_attrs = unsafe {
            let mut saved_attrs = MaybeUninit::<termios>::uninit();

            let rc = tcgetattr(stdout.as_raw_fd(), saved_attrs.as_mut_ptr());
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }

            let saved_attrs = saved_attrs.assume_init();
            let mut raw_attrs = saved_attrs;
            cfmakeraw(&mut raw_attrs);

            let rc = tcsetattr(stdout.as_raw_fd(), TCSANOW, &raw_attrs);
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }

            saved_attrs
        };

        Ok(RawTerm {
            stdin,
            stdout,
            saved_attrs,
        })
    }

    pub fn dimensions(&mut self) -> io::Result<TermDimensions> {
        let winsz = unsafe {
            let mut winsz = MaybeUninit::<winsize>::uninit();
            let rc =
                ioctl(self.stdout.as_raw_fd(), TIOCGWINSZ, winsz.as_mut_ptr());

            if rc < 0 {
                return Err(io::Error::last_os_error());
            }

            winsz.assume_init()
        };

        Ok(TermDimensions {
            rows: winsz.ws_row,
            columns: winsz.ws_col,
        })
    }
}

impl Drop for RawTerm {
    fn drop(&mut self) {
        unsafe {
            let _ =
                tcsetattr(self.stdout.as_raw_fd(), TCSANOW, &self.saved_attrs);
        }
    }
}

impl AsyncRead for RawTerm {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stdin).poll_read(cx, buf)
    }
}

impl AsyncWrite for RawTerm {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stdout).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdout).poll_flush(cx)
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdout).poll_close(cx)
    }
}
