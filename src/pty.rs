use anyhow::{bail, Result};
use libc::{
    dup2, grantpt, ioctl, ptsname_r, setsid, unlockpt, winsize, TIOCSWINSZ,
};
use std::ffi::CStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::raw::c_char;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use tokio::io::unix::AsyncFd;
use tokio::process::{Child, Command};

use crate::msgs::TermDimensions;
use crate::utils::set_nonblock;

pub struct Master {
    file: AsyncFd<File>,
}

impl Master {
    fn new(file: AsyncFd<File>) -> Self {
        Master {
            file,
        }
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.file.readable_mut().await?;

            match guard.try_io(|f| f.get_mut().read(buf)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.file.writable_mut().await?;

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

    pub fn set_dimensions(&self, dim: TermDimensions) -> io::Result<()> {
        let winsz = winsize {
            ws_row: dim.rows,
            ws_col: dim.columns,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let rc = unsafe {
            ioctl(self.file.as_raw_fd(), TIOCSWINSZ, &winsz as *const winsize)
        };

        if rc < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }
}

pub async fn spawn_shell() -> Result<(Master, Child)> {
    let master = tokio::task::spawn_blocking(|| {
        OpenOptions::new().read(true).write(true).open("/dev/ptmx")
    })
    .await
    .unwrap()?;

    let slave_path: PathBuf = unsafe {
        let master_fd = master.as_raw_fd();
        let mut buf = [0 as c_char; 1024];

        let rc = grantpt(master_fd);
        if rc < 0 {
            bail!(io::Error::last_os_error());
        }

        let rc = unlockpt(master_fd);
        if rc < 0 {
            bail!(io::Error::last_os_error());
        }

        let rc = ptsname_r(master_fd, buf.as_mut_ptr(), buf.len());
        if rc < 0 {
            bail!(io::Error::last_os_error());
        }

        let path = CStr::from_ptr(buf.as_ptr());
        path.to_str().unwrap().into()
    };

    set_nonblock(&master)?;

    let master = Master::new(AsyncFd::new(master)?);
    let child = slave_spawn_shell(&slave_path).await?;

    Ok((master, child))
}

async fn slave_spawn_shell(slave_path: &Path) -> Result<Child> {
    let slave_path = slave_path.to_owned();

    let slave = tokio::task::spawn_blocking(move || {
        OpenOptions::new().read(true).write(true).open(slave_path)
    })
    .await
    .unwrap()?;

    let slave_fd = slave.as_raw_fd();
    let mut cmd = Command::new("bash");

    unsafe {
        cmd.pre_exec(move || {
            dup2(slave_fd, 0);
            dup2(slave_fd, 1);
            dup2(slave_fd, 2);

            for fd in 3..4096 {
                libc::close(fd);
            }

            setsid();

            Ok(())
        });
    }

    Ok(cmd.spawn()?)
}
