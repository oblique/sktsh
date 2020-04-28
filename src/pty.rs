use anyhow::{bail, Result};
use libc::{dup2, grantpt, ptsname_r, setsid, unlockpt};
use std::ffi::CStr;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use smol::Async;
use std::fs::{File, OpenOptions};
use std::io;

pub type Master = Async<File>;

pub async fn spawn_shell() -> Result<(Master, Child)> {
    let master = OpenOptions::new().read(true).write(true).open("/dev/ptmx")?;

    let slave_path: PathBuf = unsafe {
        let master_fd = master.as_raw_fd();
        let mut buf = [0i8; 1024];

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

    let master = Master::new(master)?;
    let child = slave_spawn_shell(&slave_path).await?;

    Ok((master, child))
}

async fn slave_spawn_shell(slave_path: &Path) -> Result<Child> {
    let slave = OpenOptions::new().read(true).write(true).open(slave_path)?;

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
