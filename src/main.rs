#![allow(non_snake_case, non_upper_case_globals, unknown_lints, useless_format)]

extern crate log4rs;
#[macro_use]
extern crate log;

#[macro_use]
extern crate failure;

extern crate libc;

extern crate chrono;

#[cfg(test)]
#[macro_use]
extern crate maplit;

extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

pub extern crate jsonrpc_core;

pub extern crate languageserver_types;

extern crate url;

extern crate pathdiff;

extern crate diff;

extern crate regex;
pub extern crate glob;

extern crate structopt;
use structopt::StructOpt;
#[macro_use]
extern crate structopt_derive;

#[macro_use]
extern crate lazy_static;

mod logger;
mod types;
use types::*;
mod utils;
mod vim;
mod languageclient;
use languageclient::*;

#[derive(Debug, StructOpt)]
struct Opt {}

lazy_static! {
    pub static ref LOGGER: Result<log4rs::Handle> = logger::init();
}

fn run() -> Result<()> {
    let state = Arc::new(Mutex::new(State::new()));

    let stdin = std::io::stdin();
    let stdin = stdin.lock();
    state.loop_message(stdin, None)
}

fn main() {
    let version = format!(
        "{} ({} {:?})",
        env!("CARGO_PKG_VERSION"),
        option_env!("TRAVIS_COMMIT").unwrap_or("NULL"),
        chrono::Utc::now(),
    );

    let app = Opt::clap().version(version.as_str());
    let _ = app.get_matches();

    if let Err(err) = run() {
        eprintln!("{:?}", err);
    }
}
