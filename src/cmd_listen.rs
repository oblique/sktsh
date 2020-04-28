use anyhow::{Context, Result};
use futures::prelude::*;
use smol::{Async, Task};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use crate::errors::*;
use crate::pty;

pub async fn cmd_listen(path: PathBuf) -> Result<()> {
    let _ = std::fs::remove_file(&path);

    let listener = Async::<UnixListener>::bind(path)?;
    let mut incoming = listener.incoming();

    while let Some(Ok(client)) = incoming.next().await {
        Task::local(async move {
            if let Err(e) = handle_client(client).await {
                println!("{:?}", e);
            }
        })
        .detach();
    }

    Ok(())
}

async fn handle_client(mut client: Async<UnixStream>) -> Result<()> {
    let mut client_buf = [0u8; 1024];
    let mut master_buf = [0u8; 1024];

    let (mut master, mut shell_process) = pty::spawn_shell().await?;

    loop {
        futures::select! {
            // whatever we read from `client` we write it to `master`
            res = client.read(&mut client_buf).fuse() => {
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
            res = master.read(&mut master_buf).fuse() => {
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
