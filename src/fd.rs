use std::ffi::c_void;
use std::io::{Error, Read, Result, Write};
use std::os::unix::io::{AsRawFd, RawFd};

pub struct Fd(pub i32);

impl AsRawFd for Fd {
    fn as_raw_fd(&self) -> RawFd {
        self.0 as RawFd
    }
}

impl Read for Fd {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let rc = unsafe {
            libc::read(self.0, buf.as_mut_ptr() as *mut c_void, buf.len())
        };

        if rc < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }
}

impl Write for Fd {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let rc = unsafe {
            libc::write(self.0, buf.as_ptr() as *const c_void, buf.len())
        };

        if rc < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
