use super::*;
use crate::logger::Logger;
use crate::rpcclient::RpcClient;
use crate::sign::Sign;
use crate::vim::Vim;
use std::collections::BTreeMap;
use std::sync::mpsc;

pub type Fallible<T> = failure::Fallible<T>;

#[derive(Debug, Fail)]
pub enum LCError {
    #[fail(
        display = "No language server commands found for filetype: {}",
        languageId
    )]
    NoServerCommands { languageId: String },
    #[fail(display = "Language server is not running for: {}", languageId)]
    ServerNotRunning { languageId: String },
}

// Extensions.
pub const REQUEST__GetState: &str = "languageClient/getState";
pub const REQUEST__IsAlive: &str = "languageClient/isAlive";
pub const REQUEST__StartServer: &str = "languageClient/startServer";
pub const REQUEST__RegisterServerCommands: &str = "languageClient/registerServerCommands";
pub const REQUEST__OmniComplete: &str = "languageClient/omniComplete";
pub const REQUEST__SetLoggingLevel: &str = "languageClient/setLoggingLevel";
pub const REQUEST__SetDiagnosticsList: &str = "languageClient/setDiagnosticsList";
pub const REQUEST__RegisterHandlers: &str = "languageClient/registerHandlers";
pub const REQUEST__NCMRefresh: &str = "LanguageClient_NCMRefresh";
pub const REQUEST__NCM2OnComplete: &str = "LanguageClient_NCM2OnComplete";
pub const REQUEST__ExplainErrorAtPoint: &str = "languageClient/explainErrorAtPoint";
pub const REQUEST__FindLocations: &str = "languageClient/findLocations";
pub const REQUEST__DebugInfo: &str = "languageClient/debugInfo";
pub const REQUEST__CodeLensAction: &str = "LanguageClient/handleCodeLensAction";
pub const REQUEST__SemanticScopes: &str = "languageClient/semanticScopes";
pub const REQUEST__ShowSemanticHighlightSymbols: &str =
    "languageClient/showSemanticHighlightSymbols";
pub const NOTIFICATION__HandleBufNewFile: &str = "languageClient/handleBufNewFile";
pub const NOTIFICATION__HandleBufEnter: &str = "languageClient/handleBufEnter";
pub const NOTIFICATION__HandleFileType: &str = "languageClient/handleFileType";
pub const NOTIFICATION__HandleTextChanged: &str = "languageClient/handleTextChanged";
pub const NOTIFICATION__HandleBufWritePost: &str = "languageClient/handleBufWritePost";
pub const NOTIFICATION__HandleBufDelete: &str = "languageClient/handleBufDelete";
pub const NOTIFICATION__HandleCursorMoved: &str = "languageClient/handleCursorMoved";
pub const NOTIFICATION__HandleCompleteDone: &str = "languageClient/handleCompleteDone";
pub const NOTIFICATION__FZFSinkLocation: &str = "LanguageClient_FZFSinkLocation";
pub const NOTIFICATION__FZFSinkCommand: &str = "LanguageClient_FZFSinkCommand";
pub const NOTIFICATION__ServerExited: &str = "$languageClient/serverExited";
pub const NOTIFICATION__ClearDocumentHighlight: &str = "languageClient/clearDocumentHighlight";

// Extensions by language servers.
pub const NOTIFICATION__RustBeginBuild: &str = "rustDocument/beginBuild";
pub const NOTIFICATION__RustDiagnosticsBegin: &str = "rustDocument/diagnosticsBegin";
pub const NOTIFICATION__RustDiagnosticsEnd: &str = "rustDocument/diagnosticsEnd";
// This is an RLS extension but the name is general enough to assume it might be implemented by
// other language servers or planned for inclusion in the base protocol.
pub const NOTIFICATION__WindowProgress: &str = "window/progress";
pub const NOTIFICATION__LanguageStatus: &str = "language/status";
pub const REQUEST__ClassFileContents: &str = "java/classFileContents";

// Vim variable names
pub const VIM__ServerStatus: &str = "g:LanguageClient_serverStatus";
pub const VIM__ServerStatusMessage: &str = "g:LanguageClient_serverStatusMessage";
pub const VIM__IsServerRunning: &str = "LanguageClient_isServerRunning";
pub const VIM__StatusLineDiagnosticsCounts: &str = "LanguageClient_statusLineDiagnosticsCounts";

/// Thread safe read.
pub trait SyncRead: BufRead + Sync + Send + Debug {}
impl SyncRead for BufReader<ChildStdout> {}
impl SyncRead for BufReader<TcpStream> {}

/// Thread safe write.
pub trait SyncWrite: Write + Sync + Send + Debug {}
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
    MethodCall(LanguageId, rpc::MethodCall),
    Notification(LanguageId, rpc::Notification),
    Output(rpc::Output),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Call {
    MethodCall(LanguageId, rpc::MethodCall),
    Notification(LanguageId, rpc::Notification),
}

#[derive(Clone, Copy, Serialize)]
pub struct HighlightSource {
    pub buffer: Bufnr,
    pub source: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UseVirtualText {
    Diagnostics,
    CodeLens,
    All,
    No,
}

#[derive(Serialize)]
pub struct State {
    // Program state.
    #[serde(skip_serializing)]
    pub tx: crossbeam::channel::Sender<Call>,

    #[serde(skip_serializing)]
    pub clients: HashMap<LanguageId, Arc<RpcClient>>,

    #[serde(skip_serializing)]
    pub vim: Vim,

    pub capabilities: HashMap<String, Value>,
    pub registrations: Vec<Registration>,
    pub roots: HashMap<String, String>,
    pub text_documents: HashMap<String, TextDocumentItem>,
    pub text_documents_metadata: HashMap<String, TextDocumentItemMetadata>,
    pub semantic_scopes: HashMap<String, Vec<Vec<String>>>,
    pub semantic_scope_to_hl_group_table: HashMap<String, Vec<Option<String>>>,
    // filename => semantic highlight state
    pub semantic_highlights: HashMap<String, TextDocumentSemanticHighlightState>,
    // filename => diagnostics.
    pub diagnostics: HashMap<String, Vec<Diagnostic>>,
    // filename => codeLens.
    pub code_lens: HashMap<String, Vec<CodeLens>>,
    #[serde(skip_serializing)]
    pub line_diagnostics: HashMap<(String, u64), String>,
    /// Active signs.
    pub signs: HashMap<String, BTreeMap<u64, Sign>>,
    pub namespace_ids: HashMap<String, i64>,
    pub highlight_source: Option<u64>,
    pub highlights: HashMap<String, Vec<Highlight>>,
    pub highlights_placed: HashMap<String, Vec<Highlight>>,
    // TODO: make file specific.
    pub highlight_match_ids: Vec<u32>,
    pub document_highlight_source: Option<HighlightSource>,
    pub user_handlers: HashMap<String, String>,
    #[serde(skip_serializing)]
    pub watchers: HashMap<String, notify::RecommendedWatcher>,
    #[serde(skip_serializing)]
    pub watcher_rxs: HashMap<String, mpsc::Receiver<notify::DebouncedEvent>>,

    pub is_nvim: bool,
    pub last_cursor_line: u64,
    pub last_line_diagnostic: String,
    pub stashed_codeAction_actions: Vec<CodeAction>,

    // User settings.
    pub serverCommands: HashMap<String, Vec<String>>,
    // languageId => (scope_regex => highlight group)
    pub semanticHighlightMaps: HashMap<String, HashMap<String, String>>,
    pub semanticScopeSeparator: String,
    pub autoStart: bool,
    pub selectionUI: SelectionUI,
    pub selectionUI_autoOpen: bool,
    pub trace: Option<TraceOption>,
    pub diagnosticsEnable: bool,
    pub diagnosticsList: DiagnosticsList,
    pub diagnosticsDisplay: HashMap<u64, DiagnosticsDisplay>,
    pub diagnosticsSignsMax: Option<usize>,
    pub diagnostics_max_severity: DiagnosticSeverity,
    pub documentHighlightDisplay: HashMap<u64, DocumentHighlightDisplay>,
    pub windowLogMessageLevel: MessageType,
    pub settingsPath: Vec<String>,
    pub loadSettings: bool,
    pub rootMarkers: Option<RootMarkers>,
    pub change_throttle: Option<Duration>,
    pub wait_output_timeout: Duration,
    pub hoverPreview: HoverPreviewOption,
    pub completionPreferTextEdit: bool,
    pub applyCompletionAdditionalTextEdits: bool,
    pub use_virtual_text: UseVirtualText,
    pub echo_project_root: bool,

    pub serverStderr: Option<String>,
    pub logger: Logger,
    pub preferred_markup_kind: Option<Vec<MarkupKind>>,
}

impl State {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(tx: crossbeam::channel::Sender<Call>) -> Fallible<Self> {
        let logger = Logger::new()?;

        let client = Arc::new(RpcClient::new(
            None,
            BufReader::new(std::io::stdin()),
            BufWriter::new(std::io::stdout()),
            None,
            tx.clone(),
        )?);

        Ok(Self {
            tx,

            clients: hashmap! {
                None => client.clone(),
            },

            vim: Vim::new(client),

            capabilities: HashMap::new(),
            registrations: vec![],
            roots: HashMap::new(),
            text_documents: HashMap::new(),
            text_documents_metadata: HashMap::new(),
            semantic_scopes: HashMap::new(),
            semantic_scope_to_hl_group_table: HashMap::new(),
            semantic_highlights: HashMap::new(),
            code_lens: HashMap::new(),
            diagnostics: HashMap::new(),
            line_diagnostics: HashMap::new(),
            signs: HashMap::new(),
            namespace_ids: HashMap::new(),
            highlight_source: None,
            highlights: HashMap::new(),
            highlights_placed: HashMap::new(),
            highlight_match_ids: Vec::new(),
            document_highlight_source: None,
            user_handlers: HashMap::new(),
            watchers: HashMap::new(),
            watcher_rxs: HashMap::new(),

            is_nvim: false,
            last_cursor_line: 0,
            last_line_diagnostic: " ".into(),
            stashed_codeAction_actions: vec![],

            serverCommands: HashMap::new(),
            semanticHighlightMaps: HashMap::new(),
            semanticScopeSeparator: ":".into(),
            autoStart: true,
            selectionUI: SelectionUI::LocationList,
            selectionUI_autoOpen: true,
            trace: None,
            diagnosticsEnable: true,
            diagnosticsList: DiagnosticsList::Quickfix,
            diagnosticsDisplay: DiagnosticsDisplay::default(),
            diagnosticsSignsMax: None,
            diagnostics_max_severity: DiagnosticSeverity::Hint,
            documentHighlightDisplay: DocumentHighlightDisplay::default(),
            windowLogMessageLevel: MessageType::Warning,
            settingsPath: vec![format!(".vim{}settings.json", std::path::MAIN_SEPARATOR)],
            loadSettings: false,
            rootMarkers: None,
            change_throttle: None,
            wait_output_timeout: Duration::from_secs(10),
            hoverPreview: HoverPreviewOption::default(),
            completionPreferTextEdit: false,
            applyCompletionAdditionalTextEdits: true,
            use_virtual_text: UseVirtualText::All,
            echo_project_root: true,
            serverStderr: None,
            preferred_markup_kind: None,

            logger,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SelectionUI {
    FZF,
    Quickfix,
    LocationList,
}

impl Default for SelectionUI {
    fn default() -> Self {
        SelectionUI::LocationList
    }
}

impl FromStr for SelectionUI {
    type Err = Error;

    fn from_str(s: &str) -> Fallible<Self> {
        match s.to_ascii_uppercase().as_str() {
            "FZF" => Ok(SelectionUI::FZF),
            "QUICKFIX" => Ok(SelectionUI::Quickfix),
            "LOCATIONLIST" | "LOCATION-LIST" => Ok(SelectionUI::LocationList),
            _ => bail!("Invalid option for LanguageClient_selectionUI: {}", s),
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
    type Err = Error;

    fn from_str(s: &str) -> Fallible<Self> {
        match s.to_ascii_uppercase().as_str() {
            "ALWAYS" => Ok(HoverPreviewOption::Always),
            "AUTO" => Ok(HoverPreviewOption::Auto),
            "NEVER" => Ok(HoverPreviewOption::Never),
            _ => bail!("Invalid option for LanguageClient_hoverPreview: {}", s),
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
    type Err = Error;

    fn from_str(s: &str) -> Fallible<Self> {
        match s.to_ascii_uppercase().as_str() {
            "QUICKFIX" => Ok(DiagnosticsList::Quickfix),
            "LOCATION" => Ok(DiagnosticsList::Location),
            "DISABLED" => Ok(DiagnosticsList::Disabled),
            _ => bail!("Invalid option for LanguageClient_diagnosticsList: {}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsDisplay {
    pub name: String,
    pub texthl: String,
    pub signText: String,
    pub signTexthl: String,
    pub virtualTexthl: String,
}

impl DiagnosticsDisplay {
    pub fn default() -> HashMap<u64, Self> {
        let mut map = HashMap::new();
        map.insert(
            1,
            Self {
                name: "Error".to_owned(),
                texthl: "ALEError".to_owned(),
                signText: "✖".to_owned(),
                signTexthl: "ALEErrorSign".to_owned(),
                virtualTexthl: "Error".to_owned(),
            },
        );
        map.insert(
            2,
            Self {
                name: "Warning".to_owned(),
                texthl: "ALEWarning".to_owned(),
                signText: "⚠".to_owned(),
                signTexthl: "ALEWarningSign".to_owned(),
                virtualTexthl: "Todo".to_owned(),
            },
        );
        map.insert(
            3,
            Self {
                name: "Information".to_owned(),
                texthl: "ALEInfo".to_owned(),
                signText: "ℹ".to_owned(),
                signTexthl: "ALEInfoSign".to_owned(),
                virtualTexthl: "Todo".to_owned(),
            },
        );
        map.insert(
            4,
            Self {
                name: "Hint".to_owned(),
                texthl: "ALEInfo".to_owned(),
                signText: "➤".to_owned(),
                signTexthl: "ALEInfoSign".to_owned(),
                virtualTexthl: "Todo".to_owned(),
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
pub struct Highlight {
    pub line: u64,
    pub character_start: u64,
    pub character_end: u64,
    pub group: String,
    pub text: String,
}

impl PartialEq for Highlight {
    fn eq(&self, other: &Self) -> bool {
        // Quick check whether highlight should be updated.
        self.text == other.text && self.group == other.group
    }
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
    /// Deprecated. Use `user_data` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Deprecated. Use `user_data` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
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
    pub fn from_lsp(lspitem: &CompletionItem, complete_position: Option<u64>) -> Fallible<Self> {
        debug!(
            "LSP CompletionItem to VimCompleteItem: {:?}, {:?}",
            lspitem, complete_position
        );
        let abbr = lspitem.label.clone();

        let word = lspitem.insert_text.clone().unwrap_or_else(|| {
            if lspitem.insert_text_format == Some(InsertTextFormat::Snippet)
                || lspitem
                    .text_edit
                    .as_ref()
                    .map(|text_edit| text_edit.new_text.is_empty())
                    .unwrap_or(true)
            {
                return lspitem.label.clone();
            }

            match (lspitem.text_edit.clone(), complete_position) {
                (Some(ref text_edit), Some(complete_position)) => {
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
                (Some(ref text_edit), _) => text_edit.new_text.clone(),
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
    fn to_rpc_error(&self) -> rpc::Error;
}

impl ToRpcError for Error {
    fn to_rpc_error(&self) -> rpc::Error {
        rpc::Error {
            code: rpc::ErrorCode::InternalError,
            message: self.to_string(),
            data: None,
        }
    }
}

pub trait ToParams {
    fn to_params(self) -> Fallible<Params>;
}

impl<T> ToParams for T
where
    T: Serialize,
{
    fn to_params(self) -> Fallible<Params> {
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
    fn to_int(&self) -> Fallible<u64>;
}

impl<'a> ToInt for &'a str {
    fn to_int(&self) -> Fallible<u64> {
        Ok(u64::from_str(self)?)
    }
}

impl ToInt for rpc::Id {
    fn to_int(&self) -> Fallible<u64> {
        match *self {
            rpc::Id::Num(id) => Ok(id),
            rpc::Id::Str(ref s) => s.as_str().to_int(),
            rpc::Id::Null => Err(err_msg("Null id")),
        }
    }
}

pub trait ToString {
    fn to_string(&self) -> String;
}

impl ToString for lsp::MarkedString {
    fn to_string(&self) -> String {
        match *self {
            MarkedString::String(ref s) => s.clone(),
            MarkedString::LanguageString(ref ls) => ls.value.clone(),
        }
    }
}

impl ToString for lsp::MarkupContent {
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

impl ToString for lsp::Documentation {
    fn to_string(&self) -> String {
        match *self {
            lsp::Documentation::String(ref s) => s.to_owned(),
            lsp::Documentation::MarkupContent(ref mc) => mc.to_string(),
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

impl ToDisplay for lsp::MarkedString {
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

impl LinesLen for lsp::MarkedString {
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
    fn to_int(&self) -> Fallible<u64> {
        Ok(*self as u64)
    }
}

impl ToInt for MessageType {
    fn to_int(&self) -> Fallible<u64> {
        Ok(*self as u64)
    }
}

impl ToInt for DocumentHighlightKind {
    fn to_int(&self) -> Fallible<u64> {
        Ok(*self as u64)
    }
}

pub trait ToUsize {
    fn to_usize(&self) -> Fallible<usize>;
}

impl ToUsize for u64 {
    fn to_usize(&self) -> Fallible<usize> {
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
    fn filepath(&self) -> Fallible<PathBuf>;
}

impl Filepath for Url {
    fn filepath(&self) -> Fallible<PathBuf> {
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
    fn to_lsp(self) -> Fallible<T>;
}

impl ToLSP<Vec<FileEvent>> for notify::DebouncedEvent {
    fn to_lsp(self) -> Fallible<Vec<FileEvent>> {
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
            e @ notify::DebouncedEvent::Error(_, _) => Err(format_err!("{:?}", e)),
        }
    }
}

impl<T> ToLSP<T> for Value
where
    T: DeserializeOwned,
{
    fn to_lsp(self) -> Fallible<T> {
        Ok(serde_json::from_value(self)?)
    }
}

impl<T> ToLSP<T> for Option<Params>
where
    T: DeserializeOwned,
{
    fn to_lsp(self) -> Fallible<T> {
        serde_json::to_value(self)?.to_lsp()
    }
}

pub trait FromLSP<F>
where
    Self: Sized,
{
    fn from_lsp(f: &F) -> Fallible<Self>;
}

impl FromLSP<SymbolInformation> for QuickfixEntry {
    fn from_lsp(sym: &SymbolInformation) -> Fallible<Self> {
        let start = sym.location.range.start;

        Ok(Self {
            filename: sym.location.uri.filepath()?.to_string_lossy().into_owned(),
            lnum: start.line + 1,
            col: Some(start.character + 1),
            text: Some(sym.name.clone()),
            nr: None,
            typ: None,
        })
    }
}

impl FromLSP<Vec<lsp::SymbolInformation>> for Vec<QuickfixEntry> {
    fn from_lsp(symbols: &Vec<lsp::SymbolInformation>) -> Fallible<Self> {
        symbols.iter().map(QuickfixEntry::from_lsp).collect()
    }
}

impl FromLSP<Vec<lsp::DocumentSymbol>> for Vec<QuickfixEntry> {
    fn from_lsp(document_symbols: &Vec<lsp::DocumentSymbol>) -> Fallible<Self> {
        let mut symbols = Vec::new();

        fn walk_document_symbol(
            buffer: &mut Vec<QuickfixEntry>,
            parent: Option<&str>,
            ds: &lsp::DocumentSymbol,
        ) {
            let start = ds.selection_range.start;

            let name = if let Some(parent) = parent {
                format!("{}::{}", parent, ds.name)
            } else {
                ds.name.clone()
            };

            buffer.push(QuickfixEntry {
                filename: "".to_string(),
                lnum: start.line + 1,
                col: Some(start.character + 1),
                text: Some(name),
                nr: None,
                typ: None,
            });

            if let Some(children) = &ds.children {
                for child in children {
                    walk_document_symbol(buffer, Some(&ds.name), child);
                }
            }
        }

        for ds in document_symbols {
            walk_document_symbol(&mut symbols, None, ds);
        }

        Ok(symbols)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawMessage {
    Notification(rpc::Notification),
    MethodCall(rpc::MethodCall),
    Output(rpc::Output),
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct VirtualText {
    pub line: u64,
    pub text: String,
    pub hl_group: String,
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceEditWithCursor {
    pub workspaceEdit: WorkspaceEdit,
    pub cursorPosition: Option<TextDocumentPositionParams>,
}
