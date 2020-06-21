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

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use structopt::StructOpt;
use types::State;

#[derive(Debug, StructOpt)]
struct Arguments {}

fn main() -> Result<()> {
    let version = format!("{} {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"));
    let args = Arguments::clap().version(version.as_str());
    let _ = args.get_matches();

    let (tx, rx) = crossbeam::channel::unbounded();
    let language_client = language_client::LanguageClient {
        version,
        state_mutex: Arc::new(Mutex::new(State::new(tx)?)),
        clients_mutex: Arc::new(Mutex::new(HashMap::new())),
    };

    language_client.loop_call(&rx)
}
