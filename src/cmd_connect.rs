use anyhow::Result;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::raw_term::RawTerm;

pub async fn cmd_connect(path: PathBuf) -> Result<()> {
    let mut socket = UnixStream::connect(path).await?;
    let mut raw_term = RawTerm::new()?;
    let mut socket_buf = [0u8; 1024];
    let mut raw_term_buf = [0u8; 1024];

    loop {
        tokio::select! {
            // whatever we read from `socket` we write it to `raw_term`
            res = socket.read(&mut socket_buf) => {
                match res {
                    Ok(0) => break,
                    Ok(len) => raw_term
                        .write_all(&socket_buf[..len])
                        .await?,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e.into()),
                }
            }

            // whatever we read from `raw_term` we write it to `socket`
            res = raw_term.read(&mut raw_term_buf) => {
                match res {
                    Ok(0) => break,
                    Ok(len) => socket
                        .write_all(&raw_term_buf[..len])
                        .await?,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    Ok(())
}
