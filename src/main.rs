#![feature(getpid, slice_concat_ext, try_from)]
#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]
#![allow(non_snake_case, non_upper_case_globals, unknown_lints, useless_format, or_fun_call)]

extern crate chrono;
extern crate colored;
extern crate env_logger;
#[macro_use]
extern crate log;

#[macro_use]
extern crate failure;

extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

extern crate jsonrpc_core;

extern crate languageserver_types;

extern crate url;

extern crate pathdiff;

extern crate diff;

extern crate regex;

extern crate structopt;
use structopt::StructOpt;
#[macro_use]
extern crate structopt_derive;

mod logger;
mod types;
use types::*;
mod utils;
mod vim;
mod languageclient;
use languageclient::*;

#[derive(Debug, StructOpt)]
struct Opt {}

fn run() -> Result<()> {
    logger::init()?;

    let state = Arc::new(Mutex::new(State::new()));

    let stdin = std::io::stdin();
    let stdin = stdin.lock();
    state.loop_message(stdin, None)
}

fn main() {
    let _ = Opt::from_args();

    if let Err(err) = run() {
        eprintln!("{:?}", err);
    }
}
