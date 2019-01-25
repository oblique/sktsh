use std::io;

use bytes::BytesMut;
use futures::prelude::*;
use tokio::prelude::*;

pub struct Forwarder<F, T> {
    from: F,
    to: T,
    buffer: BytesMut,
}

impl<F, T> Forwarder<F, T>
where
    F: AsyncRead,
    T: AsyncWrite,
{
    pub fn new(from: F, to: T) -> Self {
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
        let mut read_closed = false;

        loop {
            self.buffer.reserve(1024);
            match self.from.read_buf(&mut self.buffer)? {
                Async::Ready(0) => {
                    // read end closed, but we may have some data in the buffer.
                    // in this case we need to return Async::Ready after we write
                    // the buffer the write end.
                    read_closed = true;
                    if self.buffer.len() > 0 {
                        break;
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
                    continue;
                }
                _ => break,
            }
        }

        match read_closed {
            true => Ok(Async::Ready(())),
            false => Ok(Async::NotReady),
        }
    }
}
