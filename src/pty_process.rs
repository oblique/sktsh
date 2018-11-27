use std::io;
use std::os::unix::io::IntoRawFd;
use std::ffi::CString;

use futures::prelude::*;
use tokio::prelude::*;
use tokio::reactor::PollEvented2;

use nix::unistd::{
    execvp, fork, setsid, dup2,
    ForkResult, Pid
};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::waitpid;

use failure::Error;
use failure::Fail;
use failure::ResultExt;

use pty;
use evented_file::EventedFile;

pub struct PtyProcess {
    master: PollEvented2<EventedFile>,
    child: Pid,
}

impl PtyProcess {
    pub fn new() -> Result<Self, Error> {
        let child_pid;
        let (master, slave) = pty::pair().unwrap();

        // deregister slave
        let slave = slave.into_inner().unwrap();

        match fork() {
            Ok(ForkResult::Parent { child }) => {
                child_pid = child;
            }
            Ok(ForkResult::Child) => {
                let slave_fd = slave.into_raw_fd();

                let redirect_and_exec = || -> Result<(), Error> {
                    // redirect stdin/stdout/stderr to slave FD
                    dup2(slave_fd, 0).context("dup2(0) failed")?;
                    dup2(slave_fd, 1).context("dup2(1) failed")?;
                    dup2(slave_fd, 2).context("dup2(2) failed")?;

                    // create new process group
                    setsid().context("setsid failed")?;

                    // exec
                    let file = CString::new("bash").unwrap();
                    let args = [ file.clone(), ];
                    execvp(&file, &args).context("Exec failed")?;

                    unreachable!();
                };

                let err = redirect_and_exec().unwrap_err();
                // we reach this point only if redirection or exec failed.
                // we must exit with error since we are the child.
                eprintln!("Error: {}", err);
                for c in err.iter_causes() {
                    eprintln!("Caused by: {}", c);
                }
                std::process::exit(1);
            }
            Err(e) => return Err(e.context("Fork failed").into()),
        }

        Ok(PtyProcess {
            master: master,
            child: child_pid,
        })
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        // TODO: use SIGTERM and try waitpid for 10 seconds, then issue SIGKILL
        let _ = kill(self.child, Signal::SIGKILL);
        let _ = waitpid(self.child, None);
    }
}

impl Write for PtyProcess {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.master.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.master.flush()
    }
}

impl AsyncWrite for PtyProcess {
    fn shutdown(&mut self) -> io::Result<Async<()>> {
        self.master.shutdown()
    }
}

impl Read for PtyProcess {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.master.read(buf)
    }
}

impl AsyncRead for PtyProcess {
}
