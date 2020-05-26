use anyhow::Result;
use std::path::PathBuf;

use crate::client::Client;

pub async fn cmd_connect(path: PathBuf) -> Result<()> {
    let mut client = Client::connect(path).await?;

    client.spawn_shell().await?;

    Ok(())
}
