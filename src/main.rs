#![recursion_limit = "256"]

use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;

mod client;
mod cmd_connect;
mod cmd_listen;
mod errors;
mod pty;
mod raw_term;
mod server;

use crate::cmd_connect::cmd_connect;
use crate::cmd_listen::cmd_listen;

#[derive(Debug, StructOpt)]
enum Opts {
    /// Start server
    Listen {
        /// Socket path
        path: PathBuf,
    },

    /// Connect to server
    Connect {
        /// Socket path
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    smol::run(async {
        let opts = Opts::from_args();

        match opts {
            Opts::Listen {
                path,
            } => cmd_listen(path).await?,

            Opts::Connect {
                path,
            } => cmd_connect(path).await?,
        }

        Ok(())
    })
}
