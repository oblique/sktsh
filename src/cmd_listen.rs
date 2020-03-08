use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::stream::StreamExt;
use tokio::task;

use crate::errors::*;
use crate::pty;

pub async fn cmd_listen(path: PathBuf) -> Result<()> {
    let _ = std::fs::remove_file(&path);

    let mut listener = UnixListener::bind(path)?;
    let mut incoming = listener.incoming();

    while let Some(Ok(client)) = incoming.next().await {
        task::spawn(async move {
            if let Err(e) = handle_client(client).await {
                println!("{:?}", e);
            }
        });
    }

    Ok(())
}

async fn handle_client(mut client: UnixStream) -> Result<()> {
    let mut client_buf = [0u8; 1024];
    let mut master_buf = [0u8; 1024];

    let (mut master, mut shell_process) = pty::spawn_shell().await?;

    loop {
        tokio::select! {
            // whatever we read from `client` we write it to `master`
            res = client.read(&mut client_buf) => {
                match res {
                    Ok(0) => break,
                    Ok(len) => master
                        .write_all(&client_buf[..len])
                        .await
                        .context(HandleClientError::ClientToPtyFailed)?,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => Err(e).context(HandleClientError::ClientToPtyFailed)?,
                }
            }

            // whatever we read from `master` we write it to `client`
            res = master.read(&mut master_buf) => {
                match res {
                    Ok(0) => break,
                    Ok(len) => client
                        .write_all(&master_buf[..len])
                        .await
                        .context(HandleClientError::PtyToClientFailed)?,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => Err(e).context(HandleClientError::PtyToClientFailed)?,
                }
            }
        }
    }

    let _ = shell_process.kill();

    Ok(())
}
