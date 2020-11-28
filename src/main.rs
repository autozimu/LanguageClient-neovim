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
use language_client::LanguageClient;
use types::State;

#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;

fn main() -> Result<()> {
    let _ = clap::app_from_crate!().get_matches();

    let (tx, rx) = crossbeam::channel::unbounded();
    let version = env!("CARGO_PKG_VERSION").into();
    let state = State::new(tx)?;
    let language_client = LanguageClient::new(version, state);
    language_client.loop_call(&rx)
}
