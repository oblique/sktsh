use anyhow::Result;
use futures::prelude::*;
use smol::Async;
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::msgs::ServerMsg;
use crate::raw_term::RawTerm;

pub struct Client {
    socket: Async<UnixStream>,
    raw_term: Option<RawTerm>,
}

impl Client {
    pub async fn connect(unix_sock_path: impl AsRef<Path>) -> Result<Self> {
        let path = unix_sock_path.as_ref();
        let socket = Async::<UnixStream>::connect(path).await?;

        Ok(Client {
            socket,
            raw_term: None,
        })
    }

    pub async fn spawn_shell(&mut self) -> Result<()> {
        let mut socket_buf = [0u8; 1024];
        let mut raw_term_buf = [0u8; 512];
        let mut signal_buf = [0u8; 1];

        self.raw_term = Some(RawTerm::new()?);
        self.send_term_dimensions().await?;

        let (tx_signal, mut rx_signal) = Async::<UnixStream>::pair()?;
        signal_hook::pipe::register(signal_hook::SIGWINCH, tx_signal)?;

        loop {
            futures::select! {
                // whatever we read from `socket` we write it to `raw_term`
                res = self.socket.read(&mut socket_buf).fuse() => match res? {
                    0 => break,
                    len => {
                        self.raw_term
                            .as_mut()
                            .unwrap()
                            .write_all(&socket_buf[..len])
                            .await?
                    }
                },

                // whatever we read from `raw_term` we write it to `socket`
                res = self
                    .raw_term
                    .as_mut()
                    .unwrap()
                    .read(&mut raw_term_buf)
                    .fuse() =>
                {
                    match res? {
                        0 => break,
                        len => {
                            let msg = ServerMsg::Data(&raw_term_buf[..len]);
                            self.send_server_msg(msg).await?;
                        }
                    }
                }

                // SIGWINCH received (i.e. terminal dimensions changed)
                res = rx_signal.read(&mut signal_buf).fuse() => match res? {
                    0 => break,
                    _ => self.send_term_dimensions().await?,
                },
            }
        }

        self.raw_term.take();

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
        if let Some(raw_term) = self.raw_term.as_mut() {
            let dim = raw_term.dimensions()?;
            let msg = ServerMsg::SetDimensions(dim);
            self.send_server_msg(msg).await?;
        }

        Ok(())
    }
}
