use crate::logger::Logger;
use crate::rpcclient::RpcClient;
use crate::{
    language_client::LanguageClient,
    utils::{code_action_kind_as_str, ToUrl},
    vim::Vim,
    watcher::FSWatch,
};
use crate::{viewport::Viewport, vim::Highlight};
use anyhow::{anyhow, Result};
use jsonrpc_core::Params;
use log::*;
use lsp_types::Range;
use lsp_types::{
    CodeAction, CodeLens, Command, CompletionItem, CompletionTextEdit, Diagnostic,
    DiagnosticSeverity, DocumentHighlightKind, FileChangeType, FileEvent, Hover, HoverContents,
    InitializeResult, InsertTextFormat, Location, MarkedString, MarkupContent, MarkupKind,
    MessageType, NumberOrString, Registration, SemanticHighlightingInformation, SymbolInformation,
    TextDocumentItem, TextDocumentPositionParams, Url, WorkspaceEdit,
};
use maplit::hashmap;
use pathdiff::diff_paths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, BufWriter, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{ChildStdin, ChildStdout},
    str::FromStr,
    sync::{mpsc, Arc},
    time::Instant,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum LSError {
    #[error("Content Modified")]
    ContentModified,
}

#[derive(Debug, Error)]
pub enum LCError {
    #[error("No language server commands found for filetype: {}", language_id)]
    NoServerCommands { language_id: String },
    #[error("Language server is not running for: {}", language_id)]
    ServerNotRunning { language_id: String },
}

pub const REQUEST_GET_STATE: &str = "languageClient/getState";
pub const REQUEST_IS_ALIVE: &str = "languageClient/isAlive";
pub const REQUEST_START_SERVER: &str = "languageClient/startServer";
pub const REQUEST_REGISTER_SERVER_COMMANDS: &str = "languageClient/registerServerCommands";
pub const REQUEST_OMNI_COMPLETE: &str = "languageClient/omniComplete";
pub const REQUEST_SET_LOGGING_LEVEL: &str = "languageClient/setLoggingLevel";
pub const REQUEST_SET_DIAGNOSTICS_LIST: &str = "languageClient/setDiagnosticsList";
pub const REQUEST_REGISTER_HANDLERS: &str = "languageClient/registerHandlers";
pub const REQUEST_NCM_REFRESH: &str = "LanguageClient_NCMRefresh";
pub const REQUEST_NCM2_ON_COMPLETE: &str = "LanguageClient_NCM2OnComplete";
pub const REQUEST_EXPLAIN_ERROR_AT_POINT: &str = "languageClient/explainErrorAtPoint";
pub const REQUEST_FIND_LOCATIONS: &str = "languageClient/findLocations";
pub const REQUEST_DEBUG_INFO: &str = "languageClient/debugInfo";
pub const REQUEST_CODE_LENS_ACTION: &str = "LanguageClient/handleCodeLensAction";
pub const REQUEST_SEMANTIC_SCOPES: &str = "languageClient/semanticScopes";
pub const REQUEST_SHOW_SEMANTIC_HL_SYMBOLS: &str = "languageClient/showSemanticHighlightSymbols";
pub const REQUEST_CLASS_FILE_CONTENTS: &str = "java/classFileContents";
pub const REQUEST_EXECUTE_CODE_ACTION: &str = "languageClient/executeCodeAction";

pub const NOTIFICATION_HANDLE_BUF_NEW_FILE: &str = "languageClient/handleBufNewFile";
pub const NOTIFICATION_HANDLE_BUF_ENTER: &str = "languageClient/handleBufEnter";
pub const NOTIFICATION_HANDLE_FILE_TYPE: &str = "languageClient/handleFileType";
pub const NOTIFICATION_HANDLE_TEXT_CHANGED: &str = "languageClient/handleTextChanged";
pub const NOTIFICATION_HANDLE_BUF_WRITE_POST: &str = "languageClient/handleBufWritePost";
pub const NOTIFICATION_HANDLE_BUF_DELETE: &str = "languageClient/handleBufDelete";
pub const NOTIFICATION_HANDLE_CURSOR_MOVED: &str = "languageClient/handleCursorMoved";
pub const NOTIFICATION_HANDLE_COMPLETE_DONE: &str = "languageClient/handleCompleteDone";
pub const NOTIFICATION_FZF_SINK_LOCATION: &str = "LanguageClient_FZFSinkLocation";
pub const NOTIFICATION_FZF_SINK_COMMAND: &str = "LanguageClient_FZFSinkCommand";
pub const NOTIFICATION_SERVER_EXITED: &str = "$languageClient/serverExited";
pub const NOTIFICATION_CLEAR_DOCUMENT_HL: &str = "languageClient/clearDocumentHighlight";
pub const NOTIFICATION_RUST_BEGIN_BUILD: &str = "rustDocument/beginBuild";
pub const NOTIFICATION_RUST_DIAGNOSTICS_BEGIN: &str = "rustDocument/diagnosticsBegin";
pub const NOTIFICATION_RUST_DIAGNOSTICS_END: &str = "rustDocument/diagnosticsEnd";
pub const NOTIFICATION_WINDOW_PROGRESS: &str = "window/progress";
pub const NOTIFICATION_LANGUAGE_STATUS: &str = "language/status";
pub const NOTIFICATION_DIAGNOSTICS_NEXT: &str = "languageClient/diagnosticsNext";
pub const NOTIFICATION_DIAGNOSTICS_PREVIOUS: &str = "languageClient/diagnosticsPrevious";

pub const VIM_SERVER_STATUS: &str = "g:LanguageClient_serverStatus";
pub const VIM_SERVER_STATUS_MESSAGE: &str = "g:LanguageClient_serverStatusMessage";
pub const VIM_IS_SERVER_RUNNING: &str = "LanguageClient_isServerRunning";
pub const VIM_STATUS_LINE_DIAGNOSTICS_COUNTS: &str = "LanguageClient_statusLineDiagnosticsCounts";

/// Thread safe read.
pub trait SyncRead: BufRead + Sync + Send + std::fmt::Debug {}
impl SyncRead for BufReader<ChildStdout> {}
impl SyncRead for BufReader<TcpStream> {}

/// Thread safe write.
pub trait SyncWrite: Write + Sync + Send + std::fmt::Debug {}
impl SyncWrite for BufWriter<ChildStdin> {}
impl SyncWrite for BufWriter<TcpStream> {}

/// Rpc message id.
pub type Id = u64;
/// Language server id.
pub type LanguageId = Option<String>;
/// Buffer id/handle.
pub type Bufnr = i64;

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    MethodCall(LanguageId, jsonrpc_core::MethodCall),
    Notification(LanguageId, jsonrpc_core::Notification),
    Output(jsonrpc_core::Output),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Call {
    MethodCall(LanguageId, jsonrpc_core::MethodCall),
    Notification(LanguageId, jsonrpc_core::Notification),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UseVirtualText {
    Diagnostics,
    CodeLens,
    All,
    No,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InlayHint {
    pub range: Range,
    pub label: String,
}

#[derive(Serialize)]
pub struct State {
    // Program state.
    #[serde(skip_serializing)]
    pub tx: crossbeam::channel::Sender<Call>,

    #[serde(skip_serializing)]
    pub clients: HashMap<LanguageId, Arc<RpcClient>>,
    #[serde(skip_serializing)]
    pub restarts: HashMap<LanguageId, u8>,

    #[serde(skip_serializing)]
    pub vim: Vim,

    pub capabilities: HashMap<String, InitializeResult>,
    pub registrations: Vec<Registration>,
    pub roots: HashMap<String, String>,
    pub text_documents: HashMap<String, TextDocumentItem>,
    pub viewports: HashMap<String, Viewport>,
    pub text_documents_metadata: HashMap<String, TextDocumentItemMetadata>,
    pub semantic_scopes: HashMap<String, Vec<Vec<String>>>,
    pub semantic_scope_to_hl_group_table: HashMap<String, Vec<Option<String>>>,
    // filename => semantic highlight state
    pub semantic_highlights: HashMap<String, TextDocumentSemanticHighlightState>,
    // filename => diagnostics.
    pub diagnostics: HashMap<String, Vec<Diagnostic>>,
    // filename => codeLens.
    pub code_lens: HashMap<String, Vec<CodeLens>>,
    // filename => inlayHint.
    pub inlay_hints: HashMap<String, Vec<InlayHint>>,
    #[serde(skip_serializing)]
    pub line_diagnostics: HashMap<(String, u64), String>,
    pub namespace_ids: HashMap<String, i64>,
    pub highlight_source: Option<u64>,
    pub highlights: HashMap<String, Vec<Highlight>>,
    pub highlights_placed: HashMap<String, Vec<Highlight>>,
    // TODO: make file specific.
    pub highlight_match_ids: Vec<u32>,
    pub user_handlers: HashMap<String, String>,
    #[serde(skip_serializing)]
    pub watchers: HashMap<String, FSWatch>,
    #[serde(skip_serializing)]
    pub watcher_rxs: HashMap<String, mpsc::Receiver<notify::DebouncedEvent>>,

    pub last_cursor_line: u64,
    pub last_line_diagnostic: String,
    pub stashed_code_action_actions: Vec<CodeAction>,

    pub logger: Logger,
    /// Stores a JSON with the initialization options for all servers started with this client, each
    /// server will store its initialization options in an object in the root of this JSON, keyed
    /// with the name of th server. So if you are running both gopls and rust-analyzer in the same
    /// instance of LanguageClient-neovim, initialization_options will look something like this:
    ///
    /// ```json
    /// {
    ///  "gopls": {  }
    ///  "rust-analyzer": {  }
    /// }
    /// ```
    ///
    /// This assumes that there are no conflicting options, but it should be a safe assumption, as
    /// servers seem to use a root section with the name of the server to group its initialization
    /// options.
    pub initialization_options: Value,
}

impl State {
    pub fn new(
        tx: crossbeam::channel::Sender<Call>,
        client: Arc<RpcClient>,
        logger: Logger,
    ) -> Self {
        Self {
            tx,
            vim: Vim::new(Arc::clone(&client)),
            clients: hashmap! { None => client },
            restarts: HashMap::new(),
            capabilities: HashMap::new(),
            registrations: vec![],
            roots: HashMap::new(),
            text_documents: HashMap::new(),
            viewports: HashMap::new(),
            text_documents_metadata: HashMap::new(),
            semantic_scopes: HashMap::new(),
            semantic_scope_to_hl_group_table: HashMap::new(),
            semantic_highlights: HashMap::new(),
            inlay_hints: HashMap::new(),
            code_lens: HashMap::new(),
            diagnostics: HashMap::new(),
            line_diagnostics: HashMap::new(),
            namespace_ids: HashMap::new(),
            highlight_source: None,
            highlights: HashMap::new(),
            highlights_placed: HashMap::new(),
            highlight_match_ids: Vec::new(),
            user_handlers: HashMap::new(),
            watchers: HashMap::new(),
            watcher_rxs: HashMap::new(),
            last_cursor_line: 0,
            last_line_diagnostic: " ".into(),
            stashed_code_action_actions: vec![],
            initialization_options: Value::Null,
            logger,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SelectionUI {
    Funcref,
    Quickfix,
    LocationList,
}

impl Default for SelectionUI {
    fn default() -> Self {
        SelectionUI::LocationList
    }
}

impl FromStr for SelectionUI {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "FUNCREF" | "FZF" => Ok(SelectionUI::Funcref),
            "QUICKFIX" => Ok(SelectionUI::Quickfix),
            "LOCATIONLIST" | "LOCATION-LIST" => Ok(SelectionUI::LocationList),
            _ => Err(anyhow!(
                "Invalid option for LanguageClient_selectionUI: {}",
                s
            )),
        }
    }
}

pub enum LCNamespace {
    VirtualText,
    SemanticHighlight,
}

impl LCNamespace {
    pub fn name(&self) -> String {
        match self {
            LCNamespace::VirtualText => "LanguageClient_VirtualText".into(),
            LCNamespace::SemanticHighlight => "LanguageClient_SemanticHighlight".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum HoverPreviewOption {
    Always,
    Auto,
    Never,
}

impl Default for HoverPreviewOption {
    fn default() -> Self {
        HoverPreviewOption::Auto
    }
}

impl FromStr for HoverPreviewOption {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "ALWAYS" => Ok(HoverPreviewOption::Always),
            "AUTO" => Ok(HoverPreviewOption::Auto),
            "NEVER" => Ok(HoverPreviewOption::Never),
            _ => Err(anyhow!(
                "Invalid option for LanguageClient_hoverPreview: {}",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DiagnosticsList {
    Quickfix,
    Location,
    Disabled,
}

impl Default for DiagnosticsList {
    fn default() -> Self {
        DiagnosticsList::Quickfix
    }
}

impl FromStr for DiagnosticsList {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "QUICKFIX" => Ok(DiagnosticsList::Quickfix),
            "LOCATION" => Ok(DiagnosticsList::Location),
            "DISABLED" => Ok(DiagnosticsList::Disabled),
            _ => Err(anyhow!(
                "Invalid option for LanguageClient_diagnosticsList: {}",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeLensDisplay {
    pub virtual_texthl: String,
}

impl Default for CodeLensDisplay {
    fn default() -> Self {
        CodeLensDisplay {
            virtual_texthl: "LanguageClientCodeLens".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsDisplay {
    pub name: String,
    pub texthl: String,
    pub sign_text: String,
    pub sign_texthl: String,
    pub virtual_texthl: String,
}

impl DiagnosticsDisplay {
    pub fn default() -> HashMap<u64, Self> {
        let mut map = HashMap::new();
        map.insert(
            1,
            Self {
                name: "Error".to_owned(),
                texthl: "LanguageClientError".to_owned(),
                sign_text: "✖".to_owned(),
                sign_texthl: "LanguageClientErrorSign".to_owned(),
                virtual_texthl: "Error".to_owned(),
            },
        );
        map.insert(
            2,
            Self {
                name: "Warning".to_owned(),
                texthl: "LanguageClientWarning".to_owned(),
                sign_text: "⚠".to_owned(),
                sign_texthl: "LanguageClientWarningSign".to_owned(),
                virtual_texthl: "Todo".to_owned(),
            },
        );
        map.insert(
            3,
            Self {
                name: "Information".to_owned(),
                texthl: "LanguageClientInfo".to_owned(),
                sign_text: "ℹ".to_owned(),
                sign_texthl: "LanguageClientInfoSign".to_owned(),
                virtual_texthl: "Todo".to_owned(),
            },
        );
        map.insert(
            4,
            Self {
                name: "Hint".to_owned(),
                texthl: "LanguageClientInfo".to_owned(),
                sign_text: "➤".to_owned(),
                sign_texthl: "LanguageClientInfoSign".to_owned(),
                virtual_texthl: "Todo".to_owned(),
            },
        );
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentHighlightDisplay {
    pub name: String,
    pub texthl: String,
}

impl DocumentHighlightDisplay {
    pub fn default() -> HashMap<u64, Self> {
        let mut map = HashMap::new();
        map.insert(
            1,
            Self {
                name: "Text".to_owned(),
                texthl: "SpellCap".to_owned(),
            },
        );
        map.insert(
            2,
            Self {
                name: "Read".to_owned(),
                texthl: "SpellLocal".to_owned(),
            },
        );
        map.insert(
            3,
            Self {
                name: "Write".to_owned(),
                texthl: "SpellRare".to_owned(),
            },
        );
        map
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentSemanticHighlightState {
    pub last_version: Option<i64>,
    pub symbols: Vec<SemanticHighlightingInformation>,
    pub highlights: Option<Vec<Highlight>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearNamespace {
    pub line_start: u64,
    pub line_end: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuickfixEntry {
    pub filename: String,
    pub lnum: u64,
    pub col: Option<u64>,
    pub nr: Option<String>,
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub typ: Option<char>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NCMInfo {
    pub name: String,
    pub abbreviation: String,
    pub enable: u64,
    pub scopes: Vec<String>,
    pub cm_refresh_patterns: Vec<String>,
    pub early_cache: u64,
    pub cm_refresh: String,
    pub priority: u64,
    pub auto_popup: u64,
    pub cm_refresh_length: u64,
    pub sort: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NCMContext {
    pub bufnr: u64,
    pub lnum: u64,
    pub col: u64,
    pub filetype: String,
    pub typed: String,
    pub filepath: String,

    pub scope: String,
    pub startcol: u64,
    pub base: String,
    pub force: u64,
    pub early_cache: bool,

    pub scope_match: String,
    pub changedtick: u64,
    pub curpos: Vec<u64>,
    pub match_end: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NCMRefreshParams {
    pub info: NCMInfo,
    pub ctx: NCMContext,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NCM2Context {
    pub bufnr: u64,
    pub lnum: u64,
    pub ccol: u64,
    pub filetype: String,
    pub typed: String,
    pub filepath: String,
    pub scope: String,
    pub startccol: u64,
    pub base: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VimCompleteItem {
    pub word: String,
    pub abbr: String,
    pub menu: String,
    pub info: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icase: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dup: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[deprecated(note = "use `user_data` instead")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[deprecated(note = "use `user_data` instead")]
    pub is_snippet: Option<bool>,
    // NOTE: `user_data` can only be string in vim. So cannot specify concrete type here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_data: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VimCompleteItemUserData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lspitem: Option<CompletionItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

impl VimCompleteItem {
    pub fn from_lsp(lspitem: &CompletionItem, complete_position: Option<u64>) -> Result<Self> {
        debug!(
            "LSP CompletionItem to VimCompleteItem: {:?}, {:?}",
            lspitem, complete_position
        );
        let abbr = lspitem.label.clone();

        if let Some(CompletionTextEdit::InsertAndReplace(_)) = lspitem.text_edit {
            error!("insert replace is not supported");
        }

        let word = lspitem.insert_text.clone().unwrap_or_else(|| {
            if lspitem.text_edit.iter().any(|te| match te {
                CompletionTextEdit::Edit(_) => false,
                CompletionTextEdit::InsertAndReplace(_) => true,
            }) {
                error!("insert replace is not supported");
            }

            if lspitem.insert_text_format == Some(InsertTextFormat::Snippet)
                || lspitem
                    .text_edit
                    .as_ref()
                    .map(|text_edit| match text_edit {
                        CompletionTextEdit::Edit(edit) => edit.new_text.is_empty(),
                        // InsertAndReplace is not supported, so if we encounter a completion item
                        // with a text edit of this variant, then we just default to using the label
                        // as the format instead of the new text.
                        CompletionTextEdit::InsertAndReplace(_) => true,
                    })
                    .unwrap_or(true)
            {
                return lspitem.label.clone();
            }

            match (lspitem.text_edit.clone(), complete_position) {
                // see comment above about InsertAndReplace
                (Some(CompletionTextEdit::InsertAndReplace(_)), _) => lspitem.label.clone(),
                (Some(CompletionTextEdit::Edit(ref text_edit)), Some(complete_position)) => {
                    // TextEdit range start might be different from vim expected completion start.
                    // From spec, TextEdit can only span one line, i.e., the current line.
                    if text_edit.range.start.character != complete_position {
                        text_edit
                            .new_text
                            .get((complete_position as usize)..)
                            .and_then(|line| line.split_whitespace().next())
                            .map_or_else(String::new, ToOwned::to_owned)
                    } else {
                        text_edit.new_text.clone()
                    }
                }
                (Some(CompletionTextEdit::Edit(ref text_edit)), _) => text_edit.new_text.clone(),
                (_, _) => lspitem.label.clone(),
            }
        });

        let snippet;
        if lspitem.insert_text_format == Some(InsertTextFormat::Snippet) {
            snippet = Some(word.clone());
        } else {
            snippet = None;
        };

        let mut info = String::new();
        if let Some(ref doc) = lspitem.documentation {
            info += &doc.to_string();
        }

        let user_data = VimCompleteItemUserData {
            lspitem: Some(lspitem.clone()),
            snippet: snippet.clone(),
        };

        #[allow(deprecated)]
        Ok(Self {
            word,
            abbr,
            icase: Some(1),
            dup: Some(1),
            menu: lspitem
                .detail
                .clone()
                .unwrap_or_default()
                .replace("\n", " "),
            info,
            kind: lspitem.kind.map(|k| format!("{:?}", k)).unwrap_or_default(),
            is_snippet: Some(snippet.is_some()),
            snippet,
            user_data: Some(serde_json::to_string(&user_data)?),
        })
    }
}

pub trait ToRpcError {
    fn to_rpc_error(&self) -> jsonrpc_core::Error;
}

impl ToRpcError for anyhow::Error {
    fn to_rpc_error(&self) -> jsonrpc_core::Error {
        jsonrpc_core::Error {
            code: jsonrpc_core::ErrorCode::InternalError,
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

pub trait ToInt {
    fn to_int(&self) -> Result<u64>;
}

impl<'a> ToInt for &'a str {
    fn to_int(&self) -> Result<u64> {
        Ok(u64::from_str(self)?)
    }
}

impl ToInt for jsonrpc_core::Id {
    fn to_int(&self) -> Result<u64> {
        match *self {
            jsonrpc_core::Id::Num(id) => Ok(id),
            jsonrpc_core::Id::Str(ref s) => s.as_str().to_int(),
            jsonrpc_core::Id::Null => Err(anyhow!("Null id")),
        }
    }
}

pub trait ToString {
    fn to_string(&self) -> String;
}

impl ToString for lsp_types::MarkedString {
    fn to_string(&self) -> String {
        match *self {
            MarkedString::String(ref s) => s.clone(),
            MarkedString::LanguageString(ref ls) => ls.value.clone(),
        }
    }
}

impl ToString for lsp_types::MarkupContent {
    fn to_string(&self) -> String {
        self.value.clone()
    }
}

impl ToString for Hover {
    fn to_string(&self) -> String {
        match self.contents {
            HoverContents::Scalar(ref ms) => ms.to_string(),
            HoverContents::Array(ref vec) => vec
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            HoverContents::Markup(ref mc) => mc.to_string(),
        }
    }
}

impl ToString for lsp_types::Documentation {
    fn to_string(&self) -> String {
        match *self {
            lsp_types::Documentation::String(ref s) => s.to_owned(),
            lsp_types::Documentation::MarkupContent(ref mc) => mc.to_string(),
        }
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

pub trait ToDisplay {
    fn to_display(&self) -> Vec<String>;
    fn vim_filetype(&self) -> Option<String> {
        None
    }
}

impl ToDisplay for lsp_types::MarkedString {
    fn to_display(&self) -> Vec<String> {
        let s = match self {
            MarkedString::String(ref s) => s,
            MarkedString::LanguageString(ref ls) => &ls.value,
        };
        s.lines().map(String::from).collect()
    }

    fn vim_filetype(&self) -> Option<String> {
        match self {
            MarkedString::String(_) => Some("markdown".to_string()),
            MarkedString::LanguageString(ref ls) => Some(ls.language.clone()),
        }
    }
}

impl ToDisplay for MarkupContent {
    fn to_display(&self) -> Vec<String> {
        self.value.lines().map(str::to_string).collect()
    }

    fn vim_filetype(&self) -> Option<String> {
        match self.kind {
            MarkupKind::Markdown => Some("markdown".to_string()),
            MarkupKind::PlainText => Some("text".to_string()),
        }
    }
}

impl ToDisplay for Hover {
    fn to_display(&self) -> Vec<String> {
        match self.contents {
            HoverContents::Scalar(ref ms) => ms.to_display(),
            HoverContents::Array(ref arr) => arr
                .iter()
                .flat_map(|ms| {
                    if let MarkedString::LanguageString(ref ls) = ms {
                        let mut buf = Vec::new();

                        buf.push(format!("```{}", ls.language));
                        buf.extend(ls.value.lines().map(String::from));
                        buf.push("```".to_string());

                        buf
                    } else {
                        ms.to_display()
                    }
                })
                .collect(),
            HoverContents::Markup(ref mc) => mc.to_display(),
        }
    }

    fn vim_filetype(&self) -> Option<String> {
        match self.contents {
            HoverContents::Scalar(ref ms) => ms.vim_filetype(),
            HoverContents::Array(_) => Some("markdown".to_string()),
            HoverContents::Markup(ref mc) => mc.vim_filetype(),
        }
    }
}

impl ToDisplay for str {
    fn to_display(&self) -> Vec<String> {
        self.lines().map(String::from).collect()
    }
}

pub trait LinesLen {
    fn lines_len(&self) -> usize;
}

impl LinesLen for lsp_types::MarkedString {
    fn lines_len(&self) -> usize {
        match *self {
            MarkedString::String(ref s) => s.lines().count(),
            MarkedString::LanguageString(ref ls) => ls.value.lines().count(),
        }
    }
}

impl LinesLen for MarkupContent {
    fn lines_len(&self) -> usize {
        self.value.lines().count()
    }
}

impl LinesLen for Hover {
    fn lines_len(&self) -> usize {
        match self.contents {
            HoverContents::Scalar(ref c) => c.lines_len(),
            HoverContents::Array(ref arr) => arr.iter().map(LinesLen::lines_len).sum(),
            HoverContents::Markup(ref c) => c.lines_len(),
        }
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
        Ok(*self as u64)
    }
}

impl ToInt for MessageType {
    fn to_int(&self) -> Result<u64> {
        Ok(*self as u64)
    }
}

impl ToInt for DocumentHighlightKind {
    fn to_int(&self) -> Result<u64> {
        Ok(*self as u64)
    }
}

pub trait ToUsize {
    fn to_usize(&self) -> Result<usize>;
}

impl ToUsize for u64 {
    fn to_usize(&self) -> Result<usize> {
        Ok(*self as usize)
    }
}

pub trait VimExp {
    fn to_key(&self) -> String;
    fn to_exp(&self) -> String;
}

impl<'a> VimExp for &'a str {
    fn to_key(&self) -> String {
        (*self).to_string()
    }

    fn to_exp(&self) -> String {
        (*self).to_string()
    }
}

impl VimExp for String {
    fn to_key(&self) -> String {
        self.clone()
    }

    fn to_exp(&self) -> String {
        self.clone()
    }
}

impl<'a> VimExp for (&'a str, &'a str) {
    fn to_key(&self) -> String {
        self.0.to_owned()
    }

    fn to_exp(&self) -> String {
        self.1.to_owned()
    }
}

impl<'a, T> VimExp for &'a [T]
where
    T: VimExp,
{
    fn to_key(&self) -> String {
        String::new()
    }

    fn to_exp(&self) -> String {
        let mut exp = "[".to_owned();
        for (i, e) in self.iter().enumerate() {
            if i != 0 {
                exp += ", ";
            }
            exp += &e.to_exp();
        }
        exp += "]";
        exp
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanguageStatusParams {
    #[serde(rename = "type")]
    pub typee: String,
    pub message: String,
}

#[derive(Debug, Eq, PartialEq, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum RootMarkers {
    Array(Vec<String>),
    Map(HashMap<String, Vec<String>>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WindowProgressParams {
    pub title: Option<String>,
    pub message: Option<String>,
    pub percentage: Option<f64>,
    pub done: Option<bool>,
}

pub trait Filepath {
    fn filepath(&self) -> Result<PathBuf>;
}

impl Filepath for Url {
    fn filepath(&self) -> Result<PathBuf> {
        self.to_file_path().or_else(|_| Ok(self.as_str().into()))
    }
}

#[derive(Debug, Serialize)]
pub struct TextDocumentItemMetadata {
    #[serde(skip_serializing)]
    pub last_change: Instant,
}

impl Default for TextDocumentItemMetadata {
    fn default() -> Self {
        Self {
            last_change: Instant::now(),
        }
    }
}

pub trait ToLSP<T> {
    fn to_lsp(self) -> Result<T>;
}

impl ToLSP<Vec<FileEvent>> for notify::DebouncedEvent {
    fn to_lsp(self) -> Result<Vec<FileEvent>> {
        match self {
            notify::DebouncedEvent::Create(p) => Ok(vec![FileEvent {
                uri: p.to_url()?,
                typ: FileChangeType::Created,
            }]),
            notify::DebouncedEvent::NoticeWrite(p) | notify::DebouncedEvent::Write(p) => {
                Ok(vec![FileEvent {
                    uri: p.to_url()?,
                    typ: FileChangeType::Changed,
                }])
            }
            notify::DebouncedEvent::NoticeRemove(p) | notify::DebouncedEvent::Remove(p) => {
                Ok(vec![FileEvent {
                    uri: p.to_url()?,
                    typ: FileChangeType::Deleted,
                }])
            }
            notify::DebouncedEvent::Rename(p1, p2) => Ok(vec![
                FileEvent {
                    uri: p1.to_url()?,
                    typ: FileChangeType::Deleted,
                },
                FileEvent {
                    uri: p2.to_url()?,
                    typ: FileChangeType::Created,
                },
            ]),
            notify::DebouncedEvent::Chmod(_) | notify::DebouncedEvent::Rescan => Ok(vec![]),
            e @ notify::DebouncedEvent::Error(_, _) => Err(anyhow!("{:?}", e)),
        }
    }
}

pub trait ListItem {
    fn quickfix_item(&self, lc: &LanguageClient) -> Result<QuickfixEntry>;
    fn string_item(&self, lc: &LanguageClient, cwd: &str) -> Result<String>;
}

impl ListItem for Location {
    fn quickfix_item(&self, lc: &LanguageClient) -> Result<QuickfixEntry> {
        let filename = self.uri.filepath()?.to_string_lossy().into_owned();
        let start = self.range.start;
        let text = lc.get_line(&filename, start.line).unwrap_or_default();

        Ok(QuickfixEntry {
            filename,
            lnum: start.line + 1,
            col: Some(start.character + 1),
            text: Some(text),
            nr: None,
            typ: None,
        })
    }

    fn string_item(&self, lc: &LanguageClient, cwd: &str) -> Result<String> {
        let filename = self.uri.filepath()?;
        let start = self.range.start;
        let text = lc.get_line(&filename, start.line).unwrap_or_default();
        let relpath = diff_paths(&filename, Path::new(&cwd)).unwrap_or(filename);
        Ok(format!(
            "{}:{}:{}:\t{}",
            relpath.to_string_lossy(),
            start.line + 1,
            start.character + 1,
            text,
        ))
    }
}

impl ListItem for CodeAction {
    fn quickfix_item(&self, _: &LanguageClient) -> Result<QuickfixEntry> {
        let text = Some(format!(
            "{}: {}",
            code_action_kind_as_str(&self),
            self.title
        ));

        Ok(QuickfixEntry {
            filename: "".into(),
            lnum: 0,
            col: None,
            text,
            nr: None,
            typ: None,
        })
    }

    fn string_item(&self, _: &LanguageClient, _: &str) -> Result<String> {
        Ok(format!(
            "{}: {}",
            code_action_kind_as_str(&self),
            self.title
        ))
    }
}

impl ListItem for Command {
    fn quickfix_item(&self, _: &LanguageClient) -> Result<QuickfixEntry> {
        Ok(QuickfixEntry {
            filename: "".into(),
            lnum: 0,
            col: None,
            text: Some(format!("{}: {}", self.command, self.title)),
            nr: None,
            typ: None,
        })
    }

    fn string_item(&self, _: &LanguageClient, _: &str) -> Result<String> {
        Ok(format!("{}: {}", self.command, self.title))
    }
}

impl ListItem for lsp_types::DocumentSymbol {
    fn quickfix_item(&self, _: &LanguageClient) -> Result<QuickfixEntry> {
        let start = self.selection_range.start;
        let result = QuickfixEntry {
            filename: "".to_string(),
            lnum: start.line + 1,
            col: Some(start.character + 1),
            text: Some(self.name.clone()),
            nr: None,
            typ: None,
        };
        Ok(result)
    }

    fn string_item(&self, _: &LanguageClient, _: &str) -> Result<String> {
        let start = self.selection_range.start;
        let result = format!(
            "{}:{}:\t{}\t\t{:?}",
            start.line + 1,
            start.character + 1,
            self.name.clone(),
            self.kind
        );
        Ok(result)
    }
}

impl ListItem for SymbolInformation {
    fn quickfix_item(&self, _: &LanguageClient) -> Result<QuickfixEntry> {
        let start = self.location.range.start;

        Ok(QuickfixEntry {
            filename: self.location.uri.filepath()?.to_string_lossy().into_owned(),
            lnum: start.line + 1,
            col: Some(start.character + 1),
            text: Some(self.name.clone()),
            nr: None,
            typ: None,
        })
    }

    fn string_item(&self, _: &LanguageClient, cwd: &str) -> Result<String> {
        let filename = self.location.uri.filepath()?;
        let relpath = diff_paths(&filename, Path::new(cwd)).unwrap_or(filename);
        let start = self.location.range.start;
        Ok(format!(
            "{}:{}:{}:\t{}\t\t{:?}",
            relpath.to_string_lossy(),
            start.line + 1,
            start.character + 1,
            self.name,
            self.kind
        ))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawMessage {
    Notification(jsonrpc_core::Notification),
    MethodCall(jsonrpc_core::MethodCall),
    Output(jsonrpc_core::Output),
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct VirtualText {
    pub line: u64,
    pub text: String,
    pub hl_group: String,
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEditWithCursor {
    pub workspace_edit: WorkspaceEdit,
    pub cursor_position: Option<TextDocumentPositionParams>,
}
