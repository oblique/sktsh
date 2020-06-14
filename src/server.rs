use anyhow::{Context, Result};
use async_dup::Arc;
use futures::future::{Fuse, FusedFuture};
use futures::prelude::*;
use smol::{Async, Task};
use std::cell::RefCell;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::pin::Pin;
use std::process::Child;

use crate::errors::*;
use crate::msgs::*;
use crate::pty;

pub struct Server {
    listener: Async<UnixListener>,
}

struct Handler {
    client: Arc<Async<UnixStream>>,
    master: Arc<pty::Master>,
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
                let handler = Handler {
                    client: Arc::new(client),
                    master: Arc::new(master),
                    shell_process,
                };

                Task::local(async move {
                    if let Err(e) = handler.handle_client().await {
                        println!("{:?}", e);
                    }
                })
                .detach();
            }
        }
    }
}

impl Handler {
    async fn handle_client(mut self) -> Result<()> {
        let client_len_buf = Arc::new(RefCell::new([0u8; 4]));
        let mut client_buf = Vec::new();
        let master_buf = Arc::new(RefCell::new([0u8; 1024]));

        let mut client_read_len_fut = Fuse::terminated();
        let mut master_read_fut = Fuse::terminated();

        loop {
            if client_read_len_fut.is_terminated() {
                let buf = client_len_buf.clone();
                let mut client_dup = self.client.clone();

                client_read_len_fut = async move {
                    let mut buf = buf.borrow_mut();
                    client_dup.read_exact(&mut buf[..]).await
                }
                .fuse();
            }

            if master_read_fut.is_terminated() {
                let buf = master_buf.clone();
                let mut master_dup = self.master.clone();

                master_read_fut = async move {
                    let mut buf = buf.borrow_mut();
                    master_dup.read(&mut buf[..]).await
                }
                .fuse();
            }

            // Safety: This is safe becasue we do not move futures before
            // their termiantion within or after the loop.
            let mut client_read_len_fut =
                unsafe { Pin::new_unchecked(&mut client_read_len_fut) };
            let mut master_read_fut =
                unsafe { Pin::new_unchecked(&mut master_read_fut) };

            futures::select! {
                // whatever we read from `client` we write it to `master`
                res = client_read_len_fut => {
                    res.context(HandleClientError::ClientToPtyFailed)?;

                    let msg_len_buf = client_len_buf.borrow().clone();
                    let msg_len = u32::from_be_bytes(msg_len_buf) as usize;

                    client_buf.resize(msg_len, 0);

                    self.client
                        .read_exact(&mut client_buf[..])
                        .await
                        .context(HandleClientError::ClientToPtyFailed)?;

                    if let Ok(msg) = bincode::deserialize(&client_buf) {
                        self.handle_msg(msg)
                            .await
                            .context(HandleClientError::ClientToPtyFailed)?;
                    }
                }

                // whatever we read from `master` we write it to `client`
                res = master_read_fut => {
                    let len =
                        res.context(HandleClientError::PtyToClientFailed)?;

                    if len == 0 {
                        break;
                    }

                    let data = &master_buf.borrow()[..len];
                    self.client
                        .write_all(data)
                        .await
                        .context(HandleClientError::PtyToClientFailed)?;
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

impl Drop for Handler {
    fn drop(&mut self) {
        let _ = self.shell_process.kill();
        let _ = self.shell_process.wait();
    }
}
