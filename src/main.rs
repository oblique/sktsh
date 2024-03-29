#![recursion_limit = "256"]

use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;

mod client;
mod errors;
mod msgs;
mod pty;
mod raw_term;
mod server;
mod utils;

use crate::client::Client;
use crate::server::Server;

#[derive(Debug, StructOpt)]
enum Opts {
    /// Start server
    Listen(ListenOpts),
    /// Connect to server
    Connect(ClientOpts),
}

#[derive(Debug, StructOpt)]
struct ListenOpts {
    /// Socket path
    path: PathBuf,
}

#[derive(Debug, StructOpt)]
struct ClientOpts {
    /// Socket path
    path: PathBuf,
}

async fn cmd_connect(opts: ClientOpts) -> Result<()> {
    let mut client = Client::connect(opts.path).await?;

    client.spawn_shell().await?;

    Ok(())
}

async fn cmd_listen(opts: ListenOpts) -> Result<()> {
    let server = Server::bind(opts.path).await?;

    server.handle_incoming_clients().await;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::from_args();

    match opts {
        Opts::Listen(opts) => cmd_listen(opts).await?,
        Opts::Connect(opts) => cmd_connect(opts).await?,
    }

    Ok(())
}
