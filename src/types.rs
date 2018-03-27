use super::*;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Fail)]
pub enum LCError {
    #[cfg_attr(rustfmt, rustfmt_skip)]
    #[fail(display = "Language server is not running for: {}", languageId)]
    NoLanguageServer { languageId: String },
}

// Extensions.
pub const REQUEST__GetState: &str = "languageClient/getState";
pub const REQUEST__IsAlive: &str = "languageClient/isAlive";
pub const REQUEST__StartServer: &str = "languageClient/startServer";
pub const REQUEST__RegisterServerCommands: &str = "languageClient/registerServerCommands";
pub const REQUEST__OmniComplete: &str = "languageClient/omniComplete";
pub const REQUEST__SetLoggingLevel: &str = "languageClient/setLoggingLevel";
pub const NOTIFICATION__HandleBufReadPost: &str = "languageClient/handleBufReadPost";
pub const NOTIFICATION__HandleTextChanged: &str = "languageClient/handleTextChanged";
pub const NOTIFICATION__HandleBufWritePost: &str = "languageClient/handleBufWritePost";
pub const NOTIFICATION__HandleBufDelete: &str = "languageClient/handleBufDelete";
pub const NOTIFICATION__HandleCursorMoved: &str = "languageClient/handleCursorMoved";
pub const NOTIFICATION__FZFSinkLocation: &str = "LanguageClient_FZFSinkLocation";
pub const NOTIFICATION__FZFSinkCommand: &str = "LanguageClient_FZFSinkCommand";
pub const NOTIFICATION__NCMRefresh: &str = "LanguageClient_NCMRefresh";

// Extensions by language servers.
pub const REQUEST__RustImplementations: &str = "rustDocument/implementations";
pub const NOTIFICATION__RustBeginBuild: &str = "rustDocument/beginBuild";
pub const NOTIFICATION__RustDiagnosticsBegin: &str = "rustDocument/diagnosticsBegin";
pub const NOTIFICATION__RustDiagnosticsEnd: &str = "rustDocument/diagnosticsEnd";
pub const REQUEST__CqueryBase: &str = "$cquery/base";
pub const REQUEST__CqueryCallers: &str = "$cquery/callers";
pub const REQUEST__CqueryDerived: &str = "$cquery/derived";
pub const REQUEST__CqueryVars: &str = "$cquery/vars";
pub const NOTIFICATION__CqueryProgress: &str = "$cquery/progress";
pub const NOTIFICATION__LanguageStatus: &str = "language/status";

pub const CommandsClient: &[&str] = &["java.apply.workspaceEdit"];

// Vim variable names
pub const VIM__ServerStatus: &str = "g:LanguageClient_serverStatus";
pub const VIM__ServerStatusMessage: &str = "g:LanguageClient_serverStatusMessage";

#[derive(Debug, Serialize)]
pub struct State {
    // Program state.
    pub id: u64,
    #[serde(skip_serializing)]
    pub txs: HashMap<u64, Sender<Result<Value>>>,
    pub child_ids: HashMap<String, u32>,
    #[serde(skip_serializing)]
    pub writers: HashMap<String, BufWriter<ChildStdin>>,
    pub capabilities: HashMap<String, Value>,
    pub roots: HashMap<String, String>,
    pub text_documents: HashMap<String, TextDocumentItem>,
    pub text_documents_metadata: HashMap<String, TextDocumentItemMetadata>,
    pub diagnostics: HashMap<String, Vec<Diagnostic>>,
    #[serde(skip_serializing)]
    pub line_diagnostics: HashMap<(String, u64), String>,
    pub signs: HashMap<String, Vec<Sign>>,
    pub highlight_source: Option<u64>,

    pub is_nvim: bool,
    pub last_cursor_line: u64,
    pub last_line_diagnostic: String,
    pub stashed_codeAction_commands: Vec<Command>,

    // User settings.
    pub serverCommands: HashMap<String, Vec<String>>,
    pub autoStart: bool,
    pub selectionUI: SelectionUI,
    pub trace: Option<TraceOption>,
    pub diagnosticsEnable: bool,
    pub diagnosticsList: DiagnosticsList,
    pub diagnosticsDisplay: HashMap<u64, DiagnosticsDisplay>,
    pub windowLogMessageLevel: MessageType,
    pub settingsPath: String,
    pub loadSettings: bool,
    pub rootMarkers: Option<RootMarkers>,
    #[serde(skip_serializing)]
    pub change_throttle: Option<Duration>,
}

impl State {
    pub fn new() -> State {
        State {
            id: 0,
            txs: HashMap::new(),
            child_ids: HashMap::new(),
            writers: HashMap::new(),
            capabilities: HashMap::new(),
            roots: HashMap::new(),
            text_documents: HashMap::new(),
            text_documents_metadata: HashMap::new(),
            diagnostics: HashMap::new(),
            line_diagnostics: HashMap::new(),
            signs: HashMap::new(),
            highlight_source: None,

            is_nvim: false,
            last_cursor_line: 0,
            last_line_diagnostic: " ".into(),
            stashed_codeAction_commands: vec![],

            serverCommands: HashMap::new(),
            autoStart: true,
            selectionUI: SelectionUI::LocationList,
            trace: None,
            diagnosticsEnable: true,
            diagnosticsList: DiagnosticsList::Quickfix,
            diagnosticsDisplay: DiagnosticsDisplay::default(),
            windowLogMessageLevel: MessageType::Warning,
            settingsPath: format!(".vim{}settings.json", std::path::MAIN_SEPARATOR),
            loadSettings: false,
            rootMarkers: None,
            change_throttle: None,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectionUI {
    FZF,
    LocationList,
}

impl Default for SelectionUI {
    fn default() -> Self {
        SelectionUI::LocationList
    }
}

impl FromStr for SelectionUI {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "FZF" => Ok(SelectionUI::FZF),
            "LOCATIONLIST" | "LOCATION-LIST" => Ok(SelectionUI::LocationList),
            _ => bail!("Invalid option for LanguageClient_selectionUI: {}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosticsList {
    Quickfix,
    Location,
}

impl Default for DiagnosticsList {
    fn default() -> Self {
        DiagnosticsList::Quickfix
    }
}

impl FromStr for DiagnosticsList {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "QUICKFIX" => Ok(DiagnosticsList::Quickfix),
            "LOCATION" => Ok(DiagnosticsList::Location),
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
    pub text: String,
    pub severity: DiagnosticSeverity,
}

impl Sign {
    pub fn new(line: u64, text: String, severity: DiagnosticSeverity) -> Sign {
        Sign {
            id: Self::get_id(line, severity),
            line,
            text,
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
        // Dirty way to check if sign has changed.
        self.text == other.text && self.severity == other.severity
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
pub struct VimCompleteItem {
    pub word: String,
    pub icase: u64,
    pub abbr: String,
    pub dup: u64,
    pub menu: String,
    pub info: String,
    pub kind: String,
    #[serde(rename = "additionalTextEdits")]
    pub additional_text_edits: Option<Vec<lsp::TextEdit>>,
}

impl From<CompletionItem> for VimCompleteItem {
    fn from(lspitem: CompletionItem) -> VimCompleteItem {
        let word = lspitem
            .insert_text
            .clone()
            .unwrap_or_else(|| lspitem.label.clone());
        let kind = match lspitem.kind {
            Some(CompletionItemKind::Variable) => "v".to_owned(),
            Some(CompletionItemKind::Method) | Some(CompletionItemKind::Function) => "f".to_owned(),
            Some(CompletionItemKind::Field) | Some(CompletionItemKind::Property) => "m".to_owned(),
            Some(CompletionItemKind::Class) => "t".to_owned(),
            Some(kind) => format!("{:?}", kind),
            None => "".to_owned(),
        };

        VimCompleteItem {
            word,
            abbr: lspitem.label.clone(),
            icase: 1,
            dup: 1,
            menu: lspitem.detail.clone().unwrap_or_default(),
            info: lspitem
                .documentation
                .map(|d| d.to_string())
                .unwrap_or_default(),
            kind,
            additional_text_edits: lspitem.additional_text_edits.clone(),
        }
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

impl ToInt for rpc::Id {
    fn to_int(&self) -> Result<u64> {
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
            HoverContents::Array(ref vec) => vec.iter()
                .map(|i| i.to_string())
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
}

impl ToDisplay for lsp::MarkedString {
    fn to_display(&self) -> Vec<String> {
        match *self {
            MarkedString::String(ref s) => s.split('\n').map(|i| i.to_string()).collect(),
            MarkedString::LanguageString(ref ls) => vec![
                format!("```{}", ls.language),
                ls.value.split('\n').map(|i| i.to_string()).collect(),
                "```".to_string(),
            ],
        }
    }
}

impl ToDisplay for MarkupContent {
    fn to_display(&self) -> Vec<String> {
        self.value.split('\n').map(|i| i.to_string()).collect()
    }
}

impl ToDisplay for Hover {
    fn to_display(&self) -> Vec<String> {
        match self.contents {
            HoverContents::Scalar(ref ms) => ms.to_display(),
            HoverContents::Array(ref arr) => arr.iter().flat_map(|i| i.to_display()).collect(),
            HoverContents::Markup(ref mc) => vec![mc.to_string()],
        }
    }
}

pub trait LinesLen {
    fn lines_len(&self) -> usize;
}

impl LinesLen for lsp::MarkedString {
    fn lines_len(&self) -> usize {
        match *self {
            MarkedString::String(ref s) => s.split('\n').count(),
            MarkedString::LanguageString(ref ls) => ls.value.split('\n').count(),
        }
    }
}

impl LinesLen for MarkupContent {
    fn lines_len(&self) -> usize {
        self.value.split('\n').count()
    }
}

impl LinesLen for Hover {
    fn lines_len(&self) -> usize {
        match self.contents {
            HoverContents::Scalar(ref c) => c.lines_len(),
            HoverContents::Array(ref arr) => arr.iter().map(|i| i.lines_len()).sum(),
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

pub trait ToUsize {
    fn to_usize(&self) -> Result<usize>;
}

impl ToUsize for u64 {
    fn to_usize(&self) -> Result<usize> {
        Ok(*self as usize)
    }
}

#[derive(Debug, PartialEq)]
pub enum VimVar {
    Buftype,
    LanguageId,
    Filename,
    Line,
    Character,
    Text,
    Cword,
    NewName,
    GotoCmd,
    Handle,
    IncludeDeclaration,
}

pub trait VimExp {
    fn to_key(&self) -> String;
    fn to_exp(&self) -> String;
}

impl VimExp for VimVar {
    fn to_key(&self) -> String {
        match *self {
            VimVar::Buftype => "buftype",
            VimVar::LanguageId => "languageId",
            VimVar::Filename => "filename",
            VimVar::Line => "line",
            VimVar::Character => "character",
            VimVar::Text => "text",
            VimVar::Cword => "cword",
            VimVar::NewName => "newName",
            VimVar::GotoCmd => "gotoCmd",
            VimVar::Handle => "handle",
            VimVar::IncludeDeclaration => "includeDeclaration",
        }.to_owned()
    }

    fn to_exp(&self) -> String {
        match *self {
            VimVar::Buftype => "&buftype",
            VimVar::LanguageId => "&filetype",
            VimVar::Filename => "s:Expand('%:p')",
            VimVar::Line => "line('.') - 1",
            VimVar::Character => "col('.') - 1",
            VimVar::Text => "getbufline('', 1, '$')",
            VimVar::Cword => "expand('<cword>')",
            VimVar::NewName | VimVar::GotoCmd => "v:null",
            VimVar::Handle | VimVar::IncludeDeclaration => "v:true",
        }.to_owned()
    }
}

impl<'a> VimExp for &'a str {
    fn to_key(&self) -> String {
        self.to_string()
    }

    fn to_exp(&self) -> String {
        self.to_string()
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
pub struct CqueryProgressParams {
    pub indexRequestCount: u64,
    pub doIdMapCount: u64,
    pub loadPreviousIndexCount: u64,
    pub onIdMappedCount: u64,
    pub onIndexedCount: u64,
}

pub trait Filepath {
    fn filepath(&self) -> Result<PathBuf>;
}

impl Filepath for Url {
    fn filepath(&self) -> Result<PathBuf> {
        self.to_file_path()
            .map_err(|_| format_err!("Uri is not valid file path: {:?}", self))
    }
}

#[derive(Debug, Serialize)]
pub struct TextDocumentItemMetadata {
    pub last_change: DateTime<Utc>,
}

impl Default for TextDocumentItemMetadata {
    fn default() -> Self {
        Self {
            last_change: Utc::now(),
        }
    }
}
