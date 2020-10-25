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
    let _ = clap::app_from_crate!().get_matches();

    let version: String = env!("CARGO_PKG_VERSION").into();
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

    let state = State::new(tx, rpcclient, logger);
    let language_client = LanguageClient::new(version, state);
    language_client.loop_call(&rx)
}
