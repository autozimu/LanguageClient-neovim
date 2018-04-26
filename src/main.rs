#![allow(non_snake_case, non_upper_case_globals)]

use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fmt::Debug;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufRead, BufReader, BufWriter};
use std::net::TcpStream;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, ChildStdout, Stdio};
use std::str::FromStr;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time;

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
use serde::de::DeserializeOwned;
use serde::Serialize;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

extern crate jsonrpc_core as rpc;
use rpc::{Params, Value};

extern crate languageserver_types as lsp;
#[allow(unused_imports)]
use lsp::notification::Notification;
use lsp::*;

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
    let mut state = State::new();

    let tx = state.tx.clone();
    let reader_thread_name: String = "reader-main".into();
    thread::Builder::new()
        .name(reader_thread_name.clone())
        .spawn(move || {
            let stdin = std::io::stdin();
            let stdin = stdin.lock();
            if let Err(err) = loop_reader(stdin, &None, &tx) {
                error!("{} exited: {:?}", reader_thread_name, err);
            }
        })?;

    state.loop_message()
}

fn main() {
    let version = format!("{} {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"),);

    let app = Opt::clap().version(version.as_str());
    let _ = app.get_matches();

    let logger = LOGGER.as_ref().map_err(|e| format_err!("{:?}", e)).unwrap();
    logger::set_logging_level(logger, "info").unwrap();

    if let Err(err) = run() {
        eprintln!("{:?}", err);
    }
}
