use anyhow::{Context, Result};
use futures::prelude::*;
use smol::{Async, Task};
use std::os::unix::net::UnixListener;
use std::path::Path;

use crate::errors::*;
use crate::pty;

pub struct Server {
    listener: Async<UnixListener>,
}

struct Handler<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    client: S,
}

impl Server {
    pub fn bind(unix_sock_path: impl AsRef<Path>) -> Result<Self> {
        let path = unix_sock_path.as_ref();

        let _ = std::fs::remove_file(path);
        let listener = Async::<UnixListener>::bind(path)?;

        Ok(Server {
            listener,
        })
    }

    pub async fn handle_incoming_clients(&self) {
        let mut incoming = self.listener.incoming();

        while let Some(Ok(client)) = incoming.next().await {
            let mut handler = Handler {
                client,
            };

            Task::spawn(async move {
                if let Err(e) = handler.handle_client().await {
                    println!("{:?}", e);
                }
            })
            .detach();
        }
    }
}

impl<S> Handler<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    async fn handle_client(&mut self) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let mut client_buf = [0u8; 1024];
        let mut master_buf = [0u8; 1024];

        let (mut master, mut shell_process) = pty::spawn_shell().await?;

        loop {
            futures::select! {
                // whatever we read from `client` we write it to `master`
                res = self.client.read(&mut client_buf).fuse() => match res {
                    Ok(0) => break,
                    Ok(len) => master
                        .write_all(&client_buf[..len])
                        .await
                        .context(HandleClientError::ClientToPtyFailed)?,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        Err(e).context(HandleClientError::ClientToPtyFailed)?
                    }
                },

                // whatever we read from `master` we write it to `client`
                res = master.read(&mut master_buf).fuse() => match res {
                    Ok(0) => break,
                    Ok(len) => self.client
                        .write_all(&master_buf[..len])
                        .await
                        .context(HandleClientError::PtyToClientFailed)?,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        Err(e).context(HandleClientError::PtyToClientFailed)?
                    }
                },
            }
        }

        let _ = shell_process.kill();

        Ok(())
    }
}
