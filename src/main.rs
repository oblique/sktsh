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

use futures::prelude::*;
use futures::future;
use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::net::{UnixListener, UnixStream};

use failure::{Fail, Error, ResultExt};

mod pty_process;
use pty_process::PtyProcess;

mod forwarder;
use forwarder::Forwarder;

mod pty;
mod raw_term;
mod evented_file;

fn display_error(err: &Error) {
    eprintln!("Error: {}", err);
    for cause in err.iter_causes() {
        eprintln!("Caused by: {}", cause);
    }
}

fn cmd_listen(matches: &ArgMatches) -> Result<(), Error> {
    let path = matches.value_of("path").unwrap().to_owned();

    let mut runtime = Runtime::new().unwrap();
    let executor = runtime.executor();

    let listener = UnixListener::bind(&path)
        .context(format!("Unable to bind UNIX socket: {}", path))?;

    let path_clone = path.clone();
    let server = listener.incoming()
        .map_err(|e| Error::from_boxed_compat(Box::new(e)))
        .for_each(move |socket| {
            println!("New connection: {:?}", socket.peer_addr().unwrap());

            let pty_process = match PtyProcess::new() {
                Ok(v) => v,
                Err(e) => {
                    let e = e.context("Failed to start pty process").into();
                    // PtyProcess failure is not fatal to the server so we just log
                    // it and close the connection by returning Ok.
                    display_error(&e);
                    return Ok(());
                }
            };

            let (pty_process_read, pty_process_write) = pty_process.split();
            let (socket_read, socket_write) = socket.split();

            let to_socket = Forwarder::new(pty_process_read, socket_write)
                .map_err(|e| e.context("Failed to forward from pty to socket").into());

            let to_pty_process = Forwarder::new(socket_read, pty_process_write)
                .map_err(|e| e.context("Failed to forward from socket to stream").into());

            let fut = to_socket.select(to_pty_process)
                .map(|_| ())
                .map_err(|(e, _): (failure::Error, _)| display_error(&e));

            executor.spawn(fut);
            Ok(())
        })
        .map_err(move |e| {
            e.context(format!("Failed to listen to: {}", path_clone)).into()
        });

    let ctrl_c = tokio_signal::ctrl_c().flatten_stream()
        .take(1).for_each(|_| Ok(()))
        .map_err(|e| e.context("SIGINT handler failed").into());

    let main_fut = server.select(ctrl_c)
        .map(|_| ())
        .map_err(|(e, _): (failure::Error, _)| e);

    println!("server listening at {}", path);
    let res = runtime.block_on(main_fut);

    // ctrl+c pressed, cleaning up
    println!("\nCleaning up");

    if let Err(e) = std::fs::remove_file(&path) {
        let e = e.context(format!("Failed to remove: {}", path)).into();
        display_error(&e);
    }

    res.context("Server failure")?;
    Ok(())
}

fn cmd_connect(matches: &ArgMatches) -> Result<(), Error> {
    let path = matches.value_of("path").unwrap().to_owned();
    let mut runtime = Runtime::new().unwrap();

    let main_fut = UnixStream::connect(&path)
        .map_err(move |e| e.context(format!("Failed to connect to: {}", path)).into())
        .and_then(|stream| -> Box<dyn Future<Item = (), Error = Error> + Send> {
            let (stream_read, stream_write) = stream.split();
            let (term_read, term_write) = match raw_term::pair() {
                Ok(v) => v,
                Err(e) => {
                    let e = e.context("Failed to initialize raw terminal");
                    return Box::new(future::err(e.into()));
                }
            };

            let to_term = Forwarder::new(stream_read, term_write)
                .map_err(|e| e.context("Failed to forward from stream to terminal").into());

            let to_stream = Forwarder::new(term_read, stream_write)
                .map_err(|e| e.context("Failed to forward from terminal to stream").into());

            let fut = to_term.select(to_stream)
                .map(|_| ())
                .map_err(|(e, _): (failure::Error, _)| e);

            Box::new(fut)
        });

    runtime.block_on(main_fut)?;
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
                         .help("UNIX socket path"))
                    .about("Starts server for listening"))
        .subcommand(SubCommand::with_name("connect")
                    .arg(Arg::with_name("path")
                         .takes_value(true)
                         .required(true)
                         .help("UNIX socket path"))
                    .about("Connect to server"))
        .get_matches();

    let res =
        match app_m.subcommand() {
            ("listen", Some(sub_m)) => cmd_listen(sub_m),
            ("connect", Some(sub_m)) => cmd_connect(sub_m),
            _ => Ok(()),
        };

    res
}

fn main() {
    if let Err(ref e) = run() {
        display_error(e);
        std::process::exit(1);
    }
}
