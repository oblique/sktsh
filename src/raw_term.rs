use libc::{
    cfmakeraw, dup, ioctl, tcgetattr, tcsetattr, termios, winsize,
    STDIN_FILENO, STDOUT_FILENO, TCSANOW, TIOCGWINSZ,
};
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use tokio::io::unix::AsyncFd;

use crate::msgs::TermDimensions;
use crate::utils::set_nonblock;

pub struct RawTerm {
    stdin: AsyncFd<File>,
    stdout: AsyncFd<File>,
    saved_attrs: termios,
}

impl RawTerm {
    pub fn new() -> io::Result<Self> {
        let stdin = unsafe {
            let fd = dup(STDIN_FILENO);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            set_nonblock(&fd)?;

            AsyncFd::new(File::from_raw_fd(fd as RawFd))?
        };

        let stdout = unsafe {
            let fd = dup(STDOUT_FILENO);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            set_nonblock(&fd)?;

            AsyncFd::new(File::from_raw_fd(fd as RawFd))?
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

    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.stdin.readable_mut().await?;

            match guard.try_io(|f| f.get_mut().read(buf)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.stdout.writable_mut().await?;

            match guard.try_io(|f| f.get_mut().write(buf)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        let mut write_len = 0;

        while write_len < buf.len() {
            let len = self.write(&buf[write_len..]).await?;
            write_len += len;
        }

        Ok(())
    }

    pub fn dimensions(&self) -> io::Result<TermDimensions> {
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
