#![recursion_limit="1024"]
extern crate failure;
#[macro_use]
extern crate clap;
extern crate futures;
extern crate tokio;
extern crate tokio_signal;
extern crate mio;
extern crate nix;
extern crate libc;
extern crate bytes;

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};

use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::net::UnixListener;

use failure::Error;
use failure::ResultExt;

mod tty_server;
use tty_server::TtyServer;

mod forwarder;
use forwarder::Forwarder;

mod pty;
mod evented_file;

fn cmd_listen(matches: &ArgMatches) -> Result<(), Error> {
    let path = matches.value_of("path").unwrap();

    let mut runtime = Runtime::new().unwrap();
    let executor = runtime.executor();

    let listener = UnixListener::bind(path)
        .context(format!("Unable to bind UNIX socket: {}", path))?;

    let server = listener.incoming().for_each(move |socket| {
        println!("New connection: {:?}", socket.peer_addr().unwrap());

        let tty_server = TtyServer::new().unwrap();
        let (tty_server_read, tty_server_write) = tty_server.split();
        let (socket_read, socket_write) = socket.split();

        let to_socket = Forwarder::new(tty_server_read, socket_write)
            .map_err(|err| println!("socket: error: {}", err));

        let to_tty_server = Forwarder::new(socket_read, tty_server_write)
            .map_err(|err| println!("tty_server: error: {}", err));

        let fut = to_socket.select(to_tty_server)
            .map(|_| ())
            .map_err(|_| ());

        executor.spawn(fut);
        Ok(())
    })
    .map_err(|err| {
        println!("Listening error: {}", err);
    });

    let ctrl_c = tokio_signal::ctrl_c().flatten_stream()
        .take(1).for_each(|_| Ok(()))
        .map_err(|err| println!("SIGINT handler error: {}", err));

    let main_fut = server.select(ctrl_c)
        .map(|_| ())
        .map_err(|_| ());

    println!("server listening at {}", path);
    let _ = runtime.block_on(main_fut);

    // ctrl+c pressed, cleaning up
    println!("\nCleaning up");

    if let Err(_) = std::fs::remove_file(path) {
        println!("Failed to remove: {}", path);
    }

    Ok(())
}

fn run() -> Result<(), Error> {
    let app_m = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .setting(AppSettings::ArgRequiredElseHelp)
        .subcommand(SubCommand::with_name("listen")
                    .arg(Arg::with_name("path")
                         .takes_value(true)
                         .required(true)
                         .help("UNIX Socket path"))
                    .about("Starts server for listening"))
        .get_matches();

    let res =
        match app_m.subcommand() {
            ("listen", Some(sub_m)) => cmd_listen(sub_m),
            _ => Ok(()),
        };

    res
}

fn main() {
    if let Err(ref e) = run() {
        eprintln!("Error: {}", e);
        for c in e.iter_causes() {
            eprintln!("Caused by: {}", c);
        }
        std::process::exit(1);
    }
}
