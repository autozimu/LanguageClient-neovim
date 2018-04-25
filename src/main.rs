#![allow(non_snake_case, non_upper_case_globals)]

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::path::{Path, PathBuf};
use std::io::prelude::*;
use std::io::{BufRead, BufReader, BufWriter};
use std::fs::File;
use std::env;
use std::process::{ChildStdin, ChildStdout, Stdio};
use std::str::FromStr;
use std::borrow::Cow;
use std::ops::Deref;
use std::fmt::Debug;
use std::net::TcpStream;

#[macro_use]
extern crate log;
extern crate log4rs;

#[macro_use]
extern crate failure;
use failure::{err_msg, Error};

extern crate libc;

extern crate chrono;
use chrono::prelude::*;
use chrono::Duration;

#[macro_use]
extern crate maplit;

extern crate serde;
use serde::Serialize;
use serde::de::DeserializeOwned;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

extern crate jsonrpc_core as rpc;
use rpc::{Params, Value};

extern crate languageserver_types as lsp;
use lsp::*;
#[allow(unused_imports)]
use lsp::notification::Notification;

extern crate url;
use url::Url;

extern crate pathdiff;
use pathdiff::diff_paths;

extern crate diff;

extern crate glob;
extern crate regex;

extern crate notify;

#[macro_use]
extern crate structopt;
use structopt::StructOpt;

#[macro_use]
extern crate lazy_static;

mod types;
use types::*;
mod utils;
use utils::*;
mod vim;
use vim::*;
mod rpchandler;
use rpchandler::*;
mod languageclient;
#[allow(unused_imports)]
use languageclient::*;
mod logger;

#[derive(Debug, StructOpt)]
struct Opt {}

lazy_static! {
    pub static ref LOGGER: Result<log4rs::Handle> = logger::init();
}

fn run() -> Result<()> {
    let state = Arc::new(RwLock::new(State::new()));

    let stdin = std::io::stdin();
    let stdin = stdin.lock();
    state.loop_message(stdin, None)
}

fn main() {
    let version = format!("{} {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"),);

    let app = Opt::clap().version(version.as_str());
    let _ = app.get_matches();

    if let Err(err) = run() {
        eprintln!("{:?}", err);
    }
}
