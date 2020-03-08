#![recursion_limit = "256"]

use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;

mod cmd_connect;
mod cmd_listen;
mod errors;
mod pty;
mod raw_term;

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
    let mut rt =
        tokio::runtime::Builder::new().threaded_scheduler().enable_all().build().unwrap();

    rt.block_on(async {
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
