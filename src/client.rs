use anyhow::Result;
use futures::prelude::*;
use smol::Async;
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::raw_term::RawTerm;

pub struct Client {
    socket: Async<UnixStream>,
}

impl Client {
    pub async fn connect(unix_sock_path: impl AsRef<Path>) -> Result<Self> {
        let path = unix_sock_path.as_ref();
        let socket = Async::<UnixStream>::connect(path).await?;

        Ok(Client {
            socket,
        })
    }

    pub async fn spawn_shell(&mut self) -> Result<()> {
        let mut raw_term = RawTerm::new()?;
        let mut socket_buf = [0u8; 1024];
        let mut raw_term_buf = [0u8; 1024];

        loop {
            futures::select! {
                // whatever we read from `socket` we write it to `raw_term`
                res = self.socket.read(&mut socket_buf).fuse() => match res {
                    Ok(0) => break,
                    Ok(len) => raw_term.write_all(&socket_buf[..len]).await?,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e.into()),
                },

                // whatever we read from `raw_term` we write it to `socket`
                res = raw_term.read(&mut raw_term_buf).fuse() => match res {
                    Ok(0) => break,
                    Ok(len) => {
                        self.socket.write_all(&raw_term_buf[..len]).await?
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e.into()),
                },
            }
        }

        Ok(())
    }
}
