use anyhow::Result;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{signal, SignalKind};

use crate::msgs::ServerMsg;
use crate::raw_term::RawTerm;

pub struct Client {
    socket: UnixStream,
    raw_term: RawTerm,
}

impl Client {
    pub async fn connect(unix_sock_path: impl AsRef<Path>) -> Result<Self> {
        let path = unix_sock_path.as_ref();

        let socket = UnixStream::connect(path).await?;
        let raw_term = RawTerm::new()?;

        Ok(Client {
            socket,
            raw_term,
        })
    }

    pub async fn spawn_shell(&mut self) -> Result<()> {
        let mut socket_buf = [0u8; 1024];
        let mut raw_term_buf = [0u8; 1024];

        let mut sigwinch = signal(SignalKind::window_change())
            .expect("Failed to register SIGWINCH");

        self.send_term_dimensions().await?;

        loop {
            tokio::select! {
                // whatever we read from `socket` we write it to `raw_term`
                res = self.socket.read(&mut socket_buf[..]) => {
                    let len = res?;

                    if len == 0 {
                        break;
                    }

                    self.raw_term.write_all(&socket_buf[..len]).await?;
                }

                // whatever we read from `raw_term` we write it to `socket`
                res = self.raw_term.read(&mut raw_term_buf[..]) => {
                    let len = res?;

                    if len == 0 {
                        break;
                    }

                    self.send_server_msg(ServerMsg::Data(&raw_term_buf[..len])).await?;
                }

                // SIGWINCH received (i.e. terminal dimensions changed)
                Some(_) = sigwinch.recv() => {
                    self.send_term_dimensions().await?;
                }
            }
        }

        Ok(())
    }

    async fn send_server_msg(&mut self, msg: ServerMsg<'_>) -> Result<()> {
        let raw_msg = bincode::serialize(&msg)?;
        let raw_len = (raw_msg.len() as u32).to_be_bytes();

        self.socket.write_all(&raw_len).await?;
        self.socket.write_all(&raw_msg).await?;

        Ok(())
    }

    async fn send_term_dimensions(&mut self) -> Result<()> {
        let dim = self.raw_term.dimensions()?;
        self.send_server_msg(ServerMsg::SetDimensions(dim)).await?;
        Ok(())
    }
}
