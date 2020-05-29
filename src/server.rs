use anyhow::{Context, Result};
use futures::prelude::*;
use smol::{Async, Task};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Child;

use crate::errors::*;
use crate::msgs::ServerMsg;
use crate::pty;

pub struct Server {
    listener: Async<UnixListener>,
}

struct Handler<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    client: S,
    master: pty::Master,
    shell_process: Child,
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
            if let Ok((master, shell_process)) = pty::spawn_shell().await {
                let mut handler = Handler {
                    client,
                    master,
                    shell_process,
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
}

impl<S> Handler<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    async fn handle_client(&mut self) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let mut client_len_buf = [0u8; 4];
        let mut client_buf = [0u8; 1024];
        let mut master_buf = [0u8; 1024];

        loop {
            futures::select! {
                // whatever we read from `client` we write it to `master`
                res = self.client.read_exact(&mut client_len_buf).fuse() => {
                    res?;

                    let msg_len = u32::from_be_bytes(client_len_buf) as usize;
                    self.client.read_exact(&mut client_buf[..msg_len]).await?;

                    if let Ok(msg) =
                        bincode::deserialize(&client_buf[..msg_len])
                    {
                        self.handle_msg(msg).await?;
                    }
                }

                // whatever we read from `master` we write it to `client`
                res = self.master.read(&mut master_buf).fuse() => match res? {
                    0 => break,
                    len => self
                        .client
                        .write_all(&master_buf[..len])
                        .await
                        .context(HandleClientError::PtyToClientFailed)?,
                },
            }
        }

        let _ = self.shell_process.kill();

        Ok(())
    }

    async fn handle_msg(&mut self, msg: ServerMsg<'_>) -> Result<()> {
        match msg {
            ServerMsg::Data(data) => self
                .master
                .write_all(data)
                .await
                .context(HandleClientError::ClientToPtyFailed)?,
        }

        Ok(())
    }
}
