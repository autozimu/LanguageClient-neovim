#![allow(non_snake_case, non_upper_case_globals)]

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::fs::{read_to_string, File};
use std::io::prelude::*;
use std::io::{BufRead, BufReader, BufWriter};
use std::net::TcpStream;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, ChildStdout, Stdio};
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use log::{debug, error, info, log_enabled, warn};

#[allow(unused_imports)]
use failure::{bail, err_msg, format_err, Error, Fail, ResultExt};

use maplit::hashmap;

use serde::de::DeserializeOwned;
use serde::Serialize;
#[macro_use]
extern crate serde_derive;
use serde_json::json;

use jsonrpc_core::{self as rpc, Params, Value};

use lsp_types::{self as lsp, *};

use url::Url;

use pathdiff::diff_paths;

use structopt::StructOpt;

mod types;
use crate::types::*;
mod utils;
use crate::utils::*;
mod language_client;
mod language_server_protocol;
mod logger;
mod rpchandler;
mod sign;
mod viewport;
mod vim;
mod vimext;

mod rpcclient;

#[derive(Debug, StructOpt)]
struct Arguments {}

fn main() -> Fallible<()> {
    let version = format!("{} {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"));
    let args = Arguments::clap().version(version.as_str());
    let _ = args.get_matches();

    let (tx, rx) = crossbeam_channel::unbounded();
    let language_client = language_client::LanguageClient(Arc::new(Mutex::new(State::new(tx)?)));

    language_client.loop_call(&rx)
}
