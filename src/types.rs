use std;
pub use std::str::FromStr;
pub use std::convert::{TryFrom, TryInto};
pub use std::collections::{HashMap, HashSet};
pub use std::sync::mpsc::{channel, Receiver, Sender};
pub use std::sync::{Arc, Mutex, MutexGuard};
pub use std::ops::Deref;
pub use std::path::{Path, PathBuf};
pub use std::io::prelude::*;
pub use std::io::{BufReader, BufWriter};
pub use std::fs::File;
pub use std::env;
pub use std::process::{ChildStdin, Stdio};
pub use jsonrpc_core::types::{Call, Error as RpcError, ErrorCode, Failure, Id, MethodCall, Notification, Output,
                              Params, Success, Value, Version};
pub use languageserver_types::*;
pub use url::Url;
pub use pathdiff::diff_paths;
pub use serde::Serialize;
pub use serde::de::DeserializeOwned;
pub use colored::Colorize;

pub use failure::Error;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Fail)]
pub enum LCError {
    #[fail(display = "Language server is not running for: {}", languageId)]
    NoLanguageServer { languageId: String },
}

// Extensions.
pub const REQUEST__Hello: &str = "hello";
pub const REQUEST__GetState: &str = "languageClient/getState";
pub const REQUEST__IsAlive: &str = "languageClient/isAlive";
pub const REQUEST__StartServer: &str = "languageClient/startServer";
pub const REQUEST__RegisterServerCommands: &str = "languageClient/registerServerCommands";
pub const REQUEST__OmniComplete: &str = "languageClient/omniComplete";
pub const NOTIFICATION__HandleBufReadPost: &str = "languageClient/handleBufReadPost";
pub const NOTIFICATION__HandleTextChanged: &str = "languageClient/handleTextChanged";
pub const NOTIFICATION__HandleBufWritePost: &str = "languageClient/handleBufWritePost";
pub const NOTIFICATION__HandleBufDelete: &str = "languageClient/handleBufDelete";
pub const NOTIFICATION__HandleCursorMoved: &str = "languageClient/handleCursorMoved";
pub const NOTIFICATION__FZFSinkLocation: &str = "LanguageClient_FZFSinkLocation";
pub const NOTIFICATION__FZFSinkCommand: &str = "LanguageClient_FZFSinkCommand";
pub const NOTIFICATION__NCMRefresh: &str = "LanguageClient_NCMRefresh";


#[derive(Debug, Serialize)]
pub struct State {
    // Program state.
    pub id: u64,
    #[serde(skip_serializing)]
    pub txs: HashMap<u64, Sender<Result<Value>>>,
    #[serde(skip_serializing)]
    pub writers: HashMap<String, BufWriter<ChildStdin>>,
    pub capabilities: HashMap<String, Value>,
    pub roots: HashMap<String, String>,
    pub text_documents: HashMap<String, TextDocumentItem>,
    pub diagnostics: HashMap<String, Vec<Diagnostic>>,
    pub line_diagnostics: HashMap<(String, u64), String>,
    pub signs: HashMap<String, Vec<Sign>>,
    pub highlight_source: Option<u64>,

    pub last_cursor_line: u64,
    pub last_line_diagnostic: String,
    pub stashed_codeAction_commands: Vec<Command>,

    // User settings.
    pub serverCommands: HashMap<String, Vec<String>>,
    pub autoStart: bool,
    pub selectionUI: SelectionUI,
    pub trace: TraceOption,
    pub diagnosticsEnable: bool,
    pub diagnosticsList: DiagnosticsList,
    pub diagnosticsDisplay: HashMap<u64, DiagnosticsDisplay>,
    pub windowLogMessageLevel: MessageType,
    pub settingsPath: String,
    pub loadSettings: bool,
}

impl State {
    pub fn new() -> State {
        State {
            id: 0,
            txs: HashMap::new(),
            writers: HashMap::new(),
            capabilities: HashMap::new(),
            roots: HashMap::new(),
            text_documents: HashMap::new(),
            diagnostics: HashMap::new(),
            line_diagnostics: HashMap::new(),
            signs: HashMap::new(),
            highlight_source: None,

            last_cursor_line: 0,
            last_line_diagnostic: " ".into(),
            stashed_codeAction_commands: vec![],

            serverCommands: HashMap::new(),
            autoStart: true,
            selectionUI: SelectionUI::LocationList,
            trace: TraceOption::Off,
            diagnosticsEnable: true,
            diagnosticsList: DiagnosticsList::Quickfix,
            diagnosticsDisplay: DiagnosticsDisplay::default(),
            windowLogMessageLevel: MessageType::Warning,
            settingsPath: format!(".vim{}settings.json", std::path::MAIN_SEPARATOR),
            loadSettings: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectionUI {
    FZF,
    LocationList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosticsList {
    Quickfix,
    Location,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsDisplay {
    pub name: String,
    pub texthl: String,
    pub signText: String,
    pub signTexthl: String,
}

impl DiagnosticsDisplay {
    pub fn default() -> HashMap<u64, DiagnosticsDisplay> {
        let mut map = HashMap::new();
        map.insert(
            1,
            DiagnosticsDisplay {
                name: "Error".to_owned(),
                texthl: "ALEError".to_owned(),
                signText: "✖".to_owned(),
                signTexthl: "ALEErrorSign".to_owned(),
            },
        );
        map.insert(
            2,
            DiagnosticsDisplay {
                name: "Warning".to_owned(),
                texthl: "ALEWarning".to_owned(),
                signText: "⚠".to_owned(),
                signTexthl: "ALEWarningSign".to_owned(),
            },
        );
        map.insert(
            3,
            DiagnosticsDisplay {
                name: "Information".to_owned(),
                texthl: "ALEInfo".to_owned(),
                signText: "ℹ".to_owned(),
                signTexthl: "ALEInfoSign".to_owned(),
            },
        );
        map.insert(
            4,
            DiagnosticsDisplay {
                name: "Hint".to_owned(),
                texthl: "ALEInfo".to_owned(),
                signText: "➤".to_owned(),
                signTexthl: "ALEInfoSign".to_owned(),
            },
        );
        map
    }
}

// Maybe with (line, character) as key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sign {
    pub id: u64,
    pub line: u64,
    pub severity: DiagnosticSeverity,
}

impl Sign {
    pub fn new(line: u64, severity: DiagnosticSeverity) -> Sign {
        Sign {
            id: Self::get_id(line, severity),
            line,
            severity,
        }
    }

    fn get_id(line: u64, severity: DiagnosticSeverity) -> u64 {
        let base_id = 75_000;
        base_id + (line - 1) * 4 + severity.to_int().unwrap_or(0) - 1
    }
}

impl std::cmp::Ord for Sign {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl std::cmp::PartialOrd for Sign {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::cmp::PartialEq for Sign {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl std::cmp::Eq for Sign {}

use std::hash::{Hash, Hasher};
impl Hash for Sign {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GotoDefinitionResponse {
    None,
    Scalar(Location),
    Array(Vec<Location>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuickfixEntry {
    pub filename: String,
    pub lnum: u64,
    pub col: Option<u64>,
    pub nr: Option<String>,
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub typee: Option<char>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NCMInfo {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NCMContext {
    pub typed: String,
    pub lnum: u64,
    pub col: u64,
    pub startcol: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompletionResult {
    Array(Vec<CompletionItem>),
    Object(CompletionList),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VimCompleteItem {
    pub word: String,
    pub icase: u64,
    pub abbr: String,
    pub dup: u64,
    pub menu: String,
    pub info: String,
    pub kind: String,
}

impl From<CompletionItem> for VimCompleteItem {
    fn from(lspitem: CompletionItem) -> VimCompleteItem {
        let word = lspitem.insert_text.clone().unwrap_or(lspitem.label.clone());
        let kind = match lspitem.kind {
            Some(CompletionItemKind::Variable) => "v".to_owned(),
            Some(CompletionItemKind::Method) | Some(CompletionItemKind::Function) => "f".to_owned(),
            Some(CompletionItemKind::Field) | Some(CompletionItemKind::Property) => "m".to_owned(),
            Some(CompletionItemKind::Class) => "f".to_owned(),
            Some(_) => format!("{:?}", lspitem.kind),
            None => "".to_owned(),
        };

        VimCompleteItem {
            word,
            abbr: lspitem.label.clone(),
            icase: 1,
            dup: 1,
            menu: lspitem.detail.clone().unwrap_or("".to_owned()),
            info: lspitem.documentation.clone().unwrap_or("".to_owned()),
            kind,
        }
    }
}

pub trait ToRpcError {
    fn to_rpc_error(&self) -> RpcError;
}

impl ToRpcError for Error {
    fn to_rpc_error(&self) -> RpcError {
        RpcError {
            code: ErrorCode::InternalError,
            message: self.to_string(),
            data: None,
        }
    }
}

pub trait ToParams {
    fn to_params(self) -> Result<Params>;
}

impl<T> ToParams for T
where
    T: Serialize,
{
    fn to_params(self) -> Result<Params> {
        use serde_json;

        let json_value = serde_json::to_value(self)?;

        let params = match json_value {
            Value::Null => Params::None,
            Value::Bool(_) | Value::Number(_) | Value::String(_) => Params::Array(vec![json_value]),
            Value::Array(vec) => Params::Array(vec),
            Value::Object(map) => Params::Map(map),
        };

        Ok(params)
    }
}

pub trait ToValue {
    fn to_value(self) -> Value;
}

impl ToValue for Option<Params> {
    fn to_value(self) -> Value {
        let params = self.unwrap_or(Params::None);

        match params {
            Params::None => Value::Null,
            Params::Array(vec) => Value::Array(vec),
            Params::Map(map) => Value::Object(map),
        }
    }
}

pub trait ToInt {
    fn to_int(&self) -> Result<u64>;
}

impl<'a> ToInt for &'a str {
    fn to_int(&self) -> Result<u64> {
        Ok(u64::from_str(self)?)
    }
}

impl ToInt for Id {
    fn to_int(&self) -> Result<u64> {
        match *self {
            Id::Num(id) => Ok(id),
            Id::Str(ref s) => s.as_str().to_int(),
            Id::Null => Err(format_err!("Null id")),
        }
    }
}

pub trait ToString {
    fn to_string(&self) -> String;
}

impl<'a> ToString for &'a str {
    fn to_string(&self) -> String {
        (*self).to_owned()
    }
}

impl ToString for Hover {
    fn to_string(&self) -> String {
        let mut message = String::new();
        let markedString_to_String = |ms: &MarkedString| -> String {
            match *ms {
                MarkedString::String(ref s) => s.clone(),
                MarkedString::LanguageString(ref ls) => ls.value.clone(),
            }
        };

        match self.contents {
            HoverContents::Scalar(ref s) => message += markedString_to_String(s).as_str(),
            HoverContents::Array(ref vec) => for item in vec {
                message += markedString_to_String(item).as_str();
            },
        };

        message
    }
}

impl ToString for NumberOrString {
    fn to_string(&self) -> String {
        match *self {
            NumberOrString::Number(n) => format!("{}", n),
            NumberOrString::String(ref s) => s.clone(),
        }
    }
}

use diff;

impl ToString for Vec<diff::Result<char>> {
    fn to_string(&self) -> String {
        let mut s = String::new();
        for comp in self {
            s += match *comp {
                diff::Result::Both(v, _) => format!("{}", v),
                diff::Result::Left(v) => format!("{}", format!("{}", v).red()),
                diff::Result::Right(v) => format!("{}", format!("{}", v).green()),
            }.as_str();
        }
        s
    }
}


pub trait DiagnosticSeverityExt {
    fn to_quickfix_entry_type(&self) -> char;
}

impl DiagnosticSeverityExt for DiagnosticSeverity {
    fn to_quickfix_entry_type(&self) -> char {
        match *self {
            DiagnosticSeverity::Error => 'E',
            DiagnosticSeverity::Warning => 'W',
            DiagnosticSeverity::Information => 'I',
            DiagnosticSeverity::Hint => 'H',
        }
    }
}

impl ToInt for DiagnosticSeverity {
    fn to_int(&self) -> Result<u64> {
        Ok(match *self {
            DiagnosticSeverity::Error => 1,
            DiagnosticSeverity::Warning => 2,
            DiagnosticSeverity::Information => 3,
            DiagnosticSeverity::Hint => 4,
        })
    }
}

pub trait ToUsize {
    fn to_usize(&self) -> Result<usize>;
}

impl ToUsize for u64 {
    fn to_usize(&self) -> Result<usize> {
        Ok((*self).try_into()?)
    }
}

#[derive(Debug, PartialEq)]
pub enum VimVarName {
    Buftype,
    LanguageId,
    Filename,
    Line,
    Character,
    Cword,
    NewName,
    Handle,
}

impl ToString for VimVarName {
    fn to_string(&self) -> String {
        match *self {
            VimVarName::Buftype => "buftype",
            VimVarName::LanguageId => "languageId",
            VimVarName::Filename => "filename",
            VimVarName::Line => "line",
            VimVarName::Character => "character",
            VimVarName::Cword => "cword",
            VimVarName::NewName => "newName",
            VimVarName::Handle => "handle",
        }.to_owned()
    }
}

pub trait ToVimExp {
    fn to_exp(&self) -> String;
}

impl<'a> ToVimExp for &'a str {
    fn to_exp(&self) -> String {
        (*self).to_owned()
    }
}

impl ToVimExp for String {
    fn to_exp(&self) -> String {
        self.clone()
    }
}

impl<'a, T> ToVimExp for &'a [T]
where
    T: ToVimExp,
{
    fn to_exp(&self) -> String {
        let mut exp = "[".to_owned();
        for (i, e) in self.iter().enumerate() {
            if i != 0 {
                exp += ", ";
            }
            exp += e.to_exp().as_str();
        }
        exp += "]";
        exp
    }
}

impl ToVimExp for VimVarName {
    fn to_exp(&self) -> String {
        match *self {
            VimVarName::Buftype => "&buftype",
            VimVarName::LanguageId => "&filetype",
            VimVarName::Filename => "s:Expand('%:p')",
            VimVarName::Line => "line('.') - 1",
            VimVarName::Character => "col('.') - 1",
            VimVarName::Cword => "expand('<cword>')",
            VimVarName::NewName => "v:null",
            VimVarName::Handle => "v:true",
        }.to_owned()
    }
}
