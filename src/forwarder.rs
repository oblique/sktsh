use std::io;

use futures::prelude::*;
use tokio::prelude::*;
use tokio::io::{ReadHalf, WriteHalf};
use bytes::BytesMut;

pub struct Forwarder<F, T> {
    from: ReadHalf<F>,
    to: WriteHalf<T>,
    buffer: BytesMut,
}

impl<F, T> Forwarder<F, T>
where
    F: AsyncRead,
    T: AsyncWrite,
{
    pub fn new(from: ReadHalf<F>, to: WriteHalf<T>) -> Self {
        Forwarder {
            from: from,
            to: to,
            buffer: BytesMut::new(),
        }
    }
}

impl<F, T> Future for Forwarder<F, T>
where
    F: AsyncRead,
    T: AsyncWrite,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            self.buffer.reserve(1024);
            match self.from.read_buf(&mut self.buffer)? {
                Async::Ready(0) => {
                    // read end closed, but we may have some data in the buffer
                    if self.buffer.len() > 0 {
                        break
                    }
                    return Ok(Async::Ready(()));
                }
                Async::Ready(_) => continue,
                _ => break,
            }
        }

        while !self.buffer.is_empty() {
            match self.to.poll_write(&mut self.buffer)? {
                Async::Ready(0) => {
                    // write end closed
                    return Ok(Async::Ready(()));
                }
                Async::Ready(n) => {
                    self.buffer.advance(n);
                    continue
                }
                _ => break,
            }
        }

        Ok(Async::NotReady)
    }
}
