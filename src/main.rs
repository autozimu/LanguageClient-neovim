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
use types::State;

#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;

fn main() -> Result<()> {
    let matches = clap::app_from_crate!()
        .arg(
            Arg::with_name("debug-locks")
                .long("debug-locks")
                .takes_value(false)
                .required(false),
        )
        .get_matches();

    let debug_locks = matches.is_present("debug-locks");
    let (tx, rx) = crossbeam::channel::unbounded();
    let version = env!("CARGO_PKG_VERSION").into();
    let state = State::new(tx, debug_locks)?;
    let language_client = LanguageClient::new(version, state);
    language_client.loop_call(&rx)
}
