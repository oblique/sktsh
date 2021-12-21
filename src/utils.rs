use std::io;
use std::os::unix::io::AsRawFd;

pub fn set_nonblock<T>(f: &T) -> io::Result<()>
where
    T: AsRawFd,
{
    let fd = f.as_raw_fd();

    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);

        if flags < 0 {
            return Err(io::Error::last_os_error());
        }

        let rc = libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);

        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}
