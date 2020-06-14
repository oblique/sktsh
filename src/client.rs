use anyhow::Result;
use async_dup::Arc;
use futures::future::{Fuse, FusedFuture};
use futures::prelude::*;
use smol::Async;
use std::cell::RefCell;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::pin::Pin;

use crate::msgs::ServerMsg;
use crate::raw_term::RawTerm;

pub struct Client {
    socket: Arc<Async<UnixStream>>,
    raw_term: Arc<RawTerm>,
}

impl Client {
    pub async fn connect(unix_sock_path: impl AsRef<Path>) -> Result<Self> {
        let path = unix_sock_path.as_ref();

        let socket = Arc::new(Async::<UnixStream>::connect(path).await?);
        let raw_term = Arc::new(RawTerm::new()?);

        Ok(Client {
            socket,
            raw_term,
        })
    }

    pub async fn spawn_shell(&mut self) -> Result<()> {
        let socket_buf = Arc::new(RefCell::new([0u8; 1024]));
        let raw_term_buf = Arc::new(RefCell::new([0u8; 1024]));
        let mut sigwinch_buf = [0u8; 1];

        let (mut sigwinch_rx, sigwinch_tx) = Async::<UnixStream>::pair()?;
        signal_hook::pipe::register(signal_hook::SIGWINCH, sigwinch_tx)?;

        let mut socket_read_fut = Fuse::terminated();
        let mut raw_term_read_fut = Fuse::terminated();
        let mut sigwinch_fut = Fuse::terminated();

        self.send_term_dimensions().await?;

        loop {
            if socket_read_fut.is_terminated() {
                let buf = socket_buf.clone();
                let mut socket_dup = self.socket.clone();

                socket_read_fut = async move {
                    let mut buf = buf.borrow_mut();
                    socket_dup.read(&mut buf[..]).await
                }
                .fuse();
            }

            if raw_term_read_fut.is_terminated() {
                let buf = raw_term_buf.clone();
                let mut raw_term_dup = self.raw_term.clone();

                raw_term_read_fut = async move {
                    let mut buf = buf.borrow_mut();
                    raw_term_dup.read(&mut buf[..]).await
                }
                .fuse();
            }

            if sigwinch_fut.is_terminated() {
                sigwinch_fut = sigwinch_rx.read_exact(&mut sigwinch_buf).fuse();
            }

            // Safety: This is safe becasue we do not move futures before
            // their termiantion within or after the loop.
            let mut socket_read_fut =
                unsafe { Pin::new_unchecked(&mut socket_read_fut) };
            let mut raw_term_read_fut =
                unsafe { Pin::new_unchecked(&mut raw_term_read_fut) };

            futures::select! {
                // whatever we read from `socket` we write it to `raw_term`
                res = socket_read_fut => {
                    let len = res?;

                    if len == 0 {
                        break;
                    }

                    let data = &socket_buf.borrow()[..len];
                    self.raw_term.write_all(data).await?;
                }

                // whatever we read from `raw_term` we write it to `socket`
                res = raw_term_read_fut => {
                    let len = res?;

                    if len == 0 {
                        break;
                    }

                    let data = &raw_term_buf.borrow()[..len];
                    self.send_server_msg(ServerMsg::Data(data)).await?;
                }

                // SIGWINCH received (i.e. terminal dimensions changed)
                res = sigwinch_fut => {
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
