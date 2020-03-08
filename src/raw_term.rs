use libc::{
    cfmakeraw, tcgetattr, tcsetattr, termios, STDIN_FILENO, STDOUT_FILENO,
    TCSANOW,
};
use std::convert::TryFrom;
use std::io;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_fd::AsyncFd;

pub struct RawTerm {
    stdin: AsyncFd,
    stdout: AsyncFd,
    saved_attrs: termios,
}

impl RawTerm {
    pub fn new() -> io::Result<Self> {
        let stdin = AsyncFd::try_from(STDIN_FILENO)?;
        let stdout = AsyncFd::try_from(STDOUT_FILENO)?;

        let saved_attrs = unsafe {
            let mut saved_attrs = MaybeUninit::<termios>::uninit();

            let rc = tcgetattr(STDOUT_FILENO, saved_attrs.as_mut_ptr());
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }

            let saved_attrs = saved_attrs.assume_init();
            let mut raw_attrs = saved_attrs;
            cfmakeraw(&mut raw_attrs);

            let rc = tcsetattr(STDOUT_FILENO, TCSANOW, &raw_attrs);
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
}

impl Drop for RawTerm {
    fn drop(&mut self) {
        unsafe {
            let _ = tcsetattr(STDOUT_FILENO, TCSANOW, &self.saved_attrs);
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

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdout).poll_shutdown(cx)
    }
}
