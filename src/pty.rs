use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{FromRawFd, IntoRawFd};

use tokio::reactor::PollEvented2;

use nix::fcntl::OFlag;
use nix::pty;

use libc;

use evented_file::EventedFile;
use errors::*;

pub fn pair() -> Result<(PollEvented2<EventedFile>, PollEvented2<EventedFile>)> {
    let mut oflags = OFlag::empty();

    // open flags for master
    oflags.insert(OFlag::O_RDWR);
    oflags.insert(OFlag::O_NONBLOCK);
    oflags.insert(OFlag::O_CLOEXEC);

    // open master pty
    let pty_master = pty::posix_openpt(oflags)?;
    pty::grantpt(&pty_master)?;
    pty::unlockpt(&pty_master)?;

    // open slave pty
    let slave_path = pty::ptsname_r(&pty_master)?;
    let pty_slave = OpenOptions::new().read(true).write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(slave_path)?;

    // get them ready for tokio
    let pty_master = PollEvented2::new(unsafe {
        EventedFile::from_raw_fd(pty_master.into_raw_fd())
    });
    let pty_slave = PollEvented2::new(EventedFile::new(pty_slave));

    Ok((pty_master, pty_slave))
}
