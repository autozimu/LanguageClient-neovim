#![allow(non_snake_case, non_upper_case_globals, unknown_lints)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Into;
use std::env;
use std::fmt::Debug;
use std::fs::{read_to_string, File};
use std::io::prelude::*;
use std::io::{BufRead, BufReader, BufWriter};
use std::net::TcpStream;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, ChildStdout, Stdio};
use std::str::FromStr;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

#[macro_use]
extern crate log;
extern crate log4rs;

#[macro_use]
extern crate failure;
use failure::{err_msg, Error};

extern crate libc;

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
use lsp::request::GotoDefinitionResponse;
use lsp::*;

extern crate url;
use url::Url;

extern crate pathdiff;
use pathdiff::diff_paths;

extern crate diff;

extern crate glob;
extern crate regex;

extern crate notify;
#[allow(unused_imports)]
use notify::Watcher;

#[macro_use]
extern crate structopt;
use structopt::StructOpt;

mod types;
use types::*;
mod utils;
use utils::*;
mod languageclient;
mod logger;
mod rpchandler;
mod vim;

#[derive(Debug, StructOpt)]
struct Arguments {}

fn main() -> Result<()> {
    let version = format!("{} {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"));
    let args = Arguments::clap().version(version.as_str());
    let _ = args.get_matches();

    let mut state = State::new()?;

    let tx = state.tx.clone();
    let reader_thread_name: String = "reader-main".into();
    thread::Builder::new()
        .name(reader_thread_name.clone())
        .spawn(move || {
            let stdin = std::io::stdin();
            let stdin = stdin.lock();
            if let Err(err) = vim::loop_reader(stdin, &None, &tx) {
                error!("{} exited: {:?}", reader_thread_name, err);
            }
        })?;

    state.loop_message()
}
