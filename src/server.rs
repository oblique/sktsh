use anyhow::{Context, Result};
use bytes::{Buf, Bytes, BytesMut};
use std::io;
use std::mem;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Child;

use crate::errors::*;
use crate::msgs::*;
use crate::pty;

pub struct Server {
    listener: UnixListener,
}

struct Handler {
    client: UnixStream,
    master: pty::Master,
    shell_process: Child,
}

impl Server {
    pub async fn bind(unix_sock_path: impl AsRef<Path>) -> Result<Self> {
        let path = unix_sock_path.as_ref();

        let _ = tokio::fs::remove_file(path).await;
        let listener = UnixListener::bind(path)?;

        Ok(Server {
            listener,
        })
    }

    pub async fn handle_incoming_clients(&self) {
        while let Ok((client, _)) = self.listener.accept().await {
            if let Ok((master, shell_process)) = pty::spawn_shell().await {
                let handler = Handler {
                    client,
                    master,
                    shell_process,
                };

                tokio::task::spawn(async move {
                    if let Err(e) = handler.handle_client().await {
                        println!("{:?}", e);
                    }
                });
            }
        }
    }
}

impl Handler {
    async fn handle_client(mut self) -> Result<()> {
        let mut client_buf = BytesMut::with_capacity(1024);
        let mut master_buf = [0u8; 1024];

        loop {
            tokio::select! {
                // whatever we read from `client` we write it to `master`
                res = read_lengthed_msg(&mut self.client, &mut client_buf) => {
                    let msg_bytes = res.context(HandleClientError::ClientToPtyFailed)?;

                    if let Ok(msg) = bincode::deserialize(&msg_bytes) {
                        self.handle_msg(msg)
                            .await
                            .context(HandleClientError::ClientToPtyFailed)?;
                    }
                }

                // whatever we read from `master` we write it to `client`
                res = self.master.read(&mut master_buf[..]) => {
                    let len = res?;

                    if len == 0 {
                        break;
                    }

                    self.client
                        .write_all(&master_buf[..len])
                        .await
                        .context(HandleClientError::PtyToClientFailed)?;
                }

                Ok(_exit_status) = self.shell_process.wait() => {
                    // TODO: Get exit status and forward it to client
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_msg(&mut self, msg: ServerMsg<'_>) -> Result<()> {
        match msg {
            ServerMsg::Data(data) => self.master.write_all(data).await?,
            ServerMsg::SetDimensions(dim) => self.master.set_dimensions(dim)?,
        }

        Ok(())
    }
}

async fn read_lengthed_msg(
    stream: &mut UnixStream,
    buf: &mut BytesMut,
) -> io::Result<Bytes> {
    loop {
        if buf.len() >= mem::size_of::<u32>() {
            let msg_len = (&buf[..]).get_u32() as usize;
            let full_frame_len = mem::size_of::<u32>() + msg_len;

            if buf.len() >= full_frame_len {
                return Ok(buf
                    .split_to(full_frame_len)
                    .split_off(mem::size_of::<u32>())
                    .freeze());
            } else {
                buf.reserve(full_frame_len - buf.len());
            }
        } else {
            buf.reserve(mem::size_of::<u32>());
        }

        if stream.read_buf(buf).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    }
}
