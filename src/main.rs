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
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use types::State;

#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;

fn main() -> Result<()> {
    let _ = clap::app_from_crate!().get_matches();

    let (tx, rx) = crossbeam::channel::unbounded();
    let language_client = language_client::LanguageClient {
        version: env!("CARGO_PKG_VERSION").into(),
        state_mutex: Arc::new(Mutex::new(State::new(tx)?)),
        clients_mutex: Arc::new(Mutex::new(HashMap::new())),
    };

    language_client.loop_call(&rx)
}
