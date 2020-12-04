mod config;
mod extensions;
mod language_client;
mod language_server_protocol;
mod logger;
mod rpcclient;
mod rpchandler;
mod sign;
mod types;
mod utils;
mod viewport;
mod vim;
mod vimext;
mod watcher;

use anyhow::Result;
use clap::Arg;
use language_client::LanguageClient;
use logger::Logger;
use rpcclient::RpcClient;
use std::{
    io::{BufReader, BufWriter},
    sync::Arc,
};
use types::{LanguageId, State};

#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;

fn main() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION").into();
    let matches = clap::app_from_crate!()
        .arg(
            Arg::with_name("debug-locks")
                .long("debug-locks")
                .takes_value(false)
                .required(false),
        )
        .get_matches();

    let debug_locks = matches.is_present("debug-locks");
    let logger = Logger::new()?;
    let (tx, rx) = crossbeam::channel::unbounded();
    let rpcclient = Arc::new(RpcClient::new(
        None,
        BufReader::new(std::io::stdin()),
        BufWriter::new(std::io::stdout()),
        None,
        tx.clone(),
        |_: &LanguageId| {},
    )?);
    if debug_locks {
        detect_deadlocks(Arc::clone(&rpcclient));
    }

    let state = State::new(tx, rpcclient, logger);
    let language_client = LanguageClient::new(version, state);
    language_client.loop_call(&rx)
}

// detect_deadlocks runs a background thread that detects deadlocks with parking_lot.
fn detect_deadlocks(client: Arc<RpcClient>) {
    use parking_lot::deadlock;
    use std::thread;
    use std::time::Duration;

    let _ = client.notify("s:Echomsg", "Deadlock detection enabled");
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(5));
        let deadlocks = deadlock::check_deadlock();
        if deadlocks.is_empty() {
            continue;
        }

        let _ = client.notify("s:Echoerr", "Deadlock detected, see logs for more info");
        log::error!("{} deadlocks detected", deadlocks.len());
        for (i, threads) in deadlocks.iter().enumerate() {
            log::error!("Deadlock #{}", i);
            for t in threads {
                log::error!("Thread Id {:#?}", t.thread_id());
                log::error!("{:#?}", t.backtrace());
            }
        }
    });
}
