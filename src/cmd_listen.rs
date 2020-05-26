use anyhow::Result;
use std::path::PathBuf;

use crate::server::Server;

pub async fn cmd_listen(path: PathBuf) -> Result<()> {
    let server = Server::bind(path)?;

    server.handle_incoming_clients().await;

    Ok(())
}
