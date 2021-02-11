mod server_command;

use itertools::Itertools;
pub use server_command::*;

use crate::{
    types::{
        CodeLensDisplay, DiagnosticsDisplay, DiagnosticsList, DocumentHighlightDisplay,
        HoverPreviewOption, RootMarkers, SelectionUI, UseVirtualText,
    },
    vim::Vim,
};
use anyhow::{anyhow, Result};
use lsp_types::{DiagnosticSeverity, MarkupKind, MessageType, TraceOption};
use serde::Deserialize;
use std::collections::HashMap;
use std::{path::PathBuf, str::FromStr, time::Duration};

#[derive(Deserialize)]
pub struct LoggerConfig {
    pub logging_file: Option<PathBuf>,
    pub logging_level: log::LevelFilter,
}

impl LoggerConfig {
    pub fn parse(vim: Vim) -> Result<Self> {
        let req = r#"{
            "logging_file": get(g:, 'LanguageClient_loggingFile', v:null),
            "logging_level": get(g:, 'LanguageClient_loggingLevel', 'WARN'),
        }"#;
        let res: LoggerConfig = vim.eval(req.replace("\n", ""))?;
        Ok(res)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTokenMapping {
    pub name: String,
    pub modifiers: Vec<String>,
    pub highlight_group: String,
}

impl SemanticTokenMapping {
    pub fn new(name: &str, modifiers: &[&str], highlight_group: &str) -> Self {
        Self {
            name: name.to_owned(),
            modifiers: modifiers.iter().map(|i| i.to_string()).collect(),
            highlight_group: highlight_group.to_owned(),
        }
    }
}

#[derive(Debug)]
pub struct Config {
    pub auto_start: bool,
    pub server_commands: HashMap<String, ServerCommand>,
    pub selection_ui: SelectionUI,
    pub trace: TraceOption,
    pub settings_path: Vec<String>,
    pub load_settings: bool,
    pub root_markers: Option<RootMarkers>,
    pub change_throttle: Option<Duration>,
    pub wait_output_timeout: Duration,
    pub diagnostics_enable: bool,
    pub diagnostics_list: DiagnosticsList,
    pub diagnostics_display: HashMap<u64, DiagnosticsDisplay>,
    pub code_lens_display: CodeLensDisplay,
    pub window_log_message_level: MessageType,
    pub hover_preview: HoverPreviewOption,
    pub completion_prefer_text_edit: bool,
    pub is_nvim: bool,
    pub server_stderr: Option<String>,
    pub diagnostics_signs_max: Option<usize>,
    pub diagnostics_max_severity: DiagnosticSeverity,
    pub diagnostics_ignore_sources: Vec<String>,
    pub document_highlight_display: HashMap<u64, DocumentHighlightDisplay>,
    pub selection_ui_auto_open: bool,
    pub use_virtual_text: UseVirtualText,
    pub echo_project_root: bool,
    pub apply_completion_text_edits: bool,
    pub preferred_markup_kind: Option<Vec<MarkupKind>>,
    pub hide_virtual_texts_on_insert: bool,
    pub enable_extensions: Option<HashMap<String, bool>>,
    pub restart_on_crash: bool,
    pub max_restart_retries: u8,
    /// semantic_token_mappings is a vec of SemanticTokenMappings, where a SemanticTokenMapping
    /// defines the token type by it's name, the modifiers and the highlight group to be applied to
    /// it.
    ///
    /// If no modifiers are configured for a type it will apply for all tokens of that type.
    ///
    /// For example:
    ///
    /// [
    ///     { "name": "function", "modifiers": ["async"], "highlightGroup": "Function" }
    ///     { "name": "type", "modifiers": [], "highlightGroup": "Type" }
    /// ]
    pub semantic_token_mappings: Vec<SemanticTokenMapping>,
    pub semantic_highlighting_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_commands: HashMap::new(),
            auto_start: true,
            selection_ui: SelectionUI::LocationList,
            selection_ui_auto_open: true,
            trace: TraceOption::default(),
            diagnostics_enable: true,
            diagnostics_list: DiagnosticsList::Quickfix,
            diagnostics_display: DiagnosticsDisplay::default(),
            code_lens_display: CodeLensDisplay::default(),
            diagnostics_signs_max: None,
            diagnostics_max_severity: DiagnosticSeverity::Hint,
            diagnostics_ignore_sources: vec![],
            document_highlight_display: DocumentHighlightDisplay::default(),
            window_log_message_level: MessageType::Warning,
            settings_path: vec![format!(".vim{}settings.json", std::path::MAIN_SEPARATOR)],
            load_settings: false,
            root_markers: None,
            change_throttle: None,
            wait_output_timeout: Duration::from_secs(10),
            hover_preview: HoverPreviewOption::default(),
            completion_prefer_text_edit: false,
            apply_completion_text_edits: true,
            use_virtual_text: UseVirtualText::All,
            hide_virtual_texts_on_insert: true,
            echo_project_root: true,
            server_stderr: None,
            preferred_markup_kind: None,
            enable_extensions: None,
            is_nvim: false,
            restart_on_crash: true,
            max_restart_retries: 5,
            semantic_token_mappings: vec![],
            semantic_highlighting_enabled: false,
        }
    }
}

#[derive(Deserialize)]
struct DeserializableConfig {
    server_stderr: Option<String>,
    auto_start: u8,
    server_commands: HashMap<String, ServerCommand>,
    selection_ui: Option<String>,
    trace: Option<String>,
    settings_path: Vec<String>,
    load_settings: u8,
    root_markers: Option<RootMarkers>,
    change_throttle: Option<f64>,
    wait_output_timeout: Option<f64>,
    diagnostics_enable: u8,
    diagnostics_list: Option<String>,
    diagnostics_display: HashMap<u64, DiagnosticsDisplay>,
    window_log_message_level: String,
    hover_preview: Option<String>,
    completion_prefer_text_edit: u8,
    is_nvim: u8,
    diagnostics_signs_max: Option<usize>,
    diagnostics_max_severity: String,
    diagnostics_ignore_sources: Vec<String>,
    document_highlight_display: Option<HashMap<u64, DocumentHighlightDisplay>>,
    selection_ui_auto_open: u8,
    use_virtual_text: UseVirtualText,
    echo_project_root: u8,
    apply_completion_text_edits: u8,
    preferred_markup_kind: Option<Vec<MarkupKind>>,
    hide_virtual_texts_on_insert: u8,
    enable_extensions: Option<HashMap<String, bool>>,
    code_lens_display: Option<CodeLensDisplay>,
    restart_on_crash: u8,
    max_restart_retries: u8,
    semantic_token_mappings: Vec<SemanticTokenMapping>,
    semantic_highlighting_enabled: u8,
}

impl Config {
    pub fn parse(vim: Vim) -> Result<Self> {
        let req = r#"{
            "auto_start": !!get(g:, 'LanguageClient_autoStart', 1),
            "server_commands": s:GetVar('LanguageClient_serverCommands', {}),
            "selection_ui": s:getSelectionUI(),
            "trace": get(g:, 'LanguageClient_trace', v:null),
            "settings_path": map(s:ToList(get(g:, 'LanguageClient_settingsPath', '.vim/settings.json')), 'expand(v:val)'),
            "load_settings": !!get(g:, 'LanguageClient_loadSettings', 1),
            "root_markers": get(g:, 'LanguageClient_rootMarkers', v:null),
            "change_throttle": get(g:, 'LanguageClient_changeThrottle', v:null),
            "wait_output_timeout": get(g:, 'LanguageClient_waitOutputTimeout', v:null),
            "diagnostics_enable": !!get(g:, 'LanguageClient_diagnosticsEnable', 1),
            "diagnostics_list": get(g:, 'LanguageClient_diagnosticsList', 'Quickfix'),
            "diagnostics_display": get(g:, 'LanguageClient_diagnosticsDisplay', {}),
            "window_log_message_level": get(g:, 'LanguageClient_windowLogMessageLevel', 'Warning'),
            "hover_preview": get(g:, 'LanguageClient_hoverPreview', 'Auto'),
            "completion_prefer_text_edit": get(g:, 'LanguageClient_completionPreferTextEdit', 0),
            "is_nvim": has('nvim'),
            "diagnostics_signs_max": get(g:, 'LanguageClient_diagnosticsSignsMax', v:null),
            "diagnostics_max_severity": get(g:, 'LanguageClient_diagnosticsMaxSeverity', 'Hint'),
            "diagnostics_ignore_sources": get(g:, 'LanguageClient_diagnosticsIgnoreSources', []),
            "document_highlight_display": get(g:, 'LanguageClient_documentHighlightDisplay', {}),
            "selection_ui_auto_open": !!s:GetVar('LanguageClient_selectionUI_autoOpen', 1),
            "use_virtual_text": s:useVirtualText(),
            "echo_project_root": !!s:GetVar('LanguageClient_echoProjectRoot', 1),
            "apply_completion_text_edits": get(g:, 'LanguageClient_applyCompletionAdditionalTextEdits', 1),
            "preferred_markup_kind": get(g:, 'LanguageClient_preferredMarkupKind', v:null),
            "hide_virtual_texts_on_insert": s:GetVar('LanguageClient_hideVirtualTextsOnInsert', 0),
            "enable_extensions": get(g:, 'LanguageClient_enableExtensions', v:null),
            "code_lens_display": get(g:, 'LanguageClient_codeLensDisplay', v:null),
            "restart_on_crash": get(g:, 'LanguageClient_restartOnCrash', 1),
            "max_restart_retries": get(g:, 'LanguageClient_maxRestartRetries', 5),
            "server_stderr": get(g:, 'LanguageClient_serverStderr', v:null),
            "semantic_token_mappings": get(g:, 'LanguageClient_semanticTokenMappings', []),
            "semantic_highlighting_enabled": get(g:, 'LanguageClient_semanticHighlightingEnabled', 0),
        }"#;

        let res: DeserializableConfig = vim.eval(req.replace("\n", ""))?;
        let mut default_mappings = vec![
            SemanticTokenMapping::new("type", &["declaration"], "Type"),
            SemanticTokenMapping::new("class", &["declaration"], "Structure"),
            SemanticTokenMapping::new("enum", &["declaration"], "Structure"),
            SemanticTokenMapping::new("interface", &["declaration"], "Structure"),
            SemanticTokenMapping::new("struct", &["declaration"], "Structure"),
            // SemanticTokenMapping::new("typeParameter", &[], "Typedef"),
            // SemanticTokenMapping::new("parameter", &[], "Identifier"),
            // SemanticTokenMapping::new("variable", &[], "Identifier"),
            // SemanticTokenMapping::new("property", &[], "Typedef"),
            // SemanticTokenMapping::new("enumMember", &[], "Typedef"),
            // SemanticTokenMapping::new("event", &[], "Typedef"),
            SemanticTokenMapping::new("function", &[], "Function"),
            SemanticTokenMapping::new("method", &[], "Function"),
            SemanticTokenMapping::new("macro", &[], "Function"),
            SemanticTokenMapping::new("function", &["deprecated"], "Comment"),
            SemanticTokenMapping::new("method", &["deprecated"], "Comment"),
            SemanticTokenMapping::new("macro", &["deprecated"], "Comment"),
            SemanticTokenMapping::new("keyword", &[], "Keyword"),
            // SemanticTokenMapping::new("modifier", &[], "Function"),
            SemanticTokenMapping::new("comment", &[], "Comment"),
            SemanticTokenMapping::new("comment", &["documentation"], "Special"),
            SemanticTokenMapping::new("string", &[], "String"),
            SemanticTokenMapping::new("number", &[], "Number"),
            SemanticTokenMapping::new("regexp", &[], "String"),
            SemanticTokenMapping::new("operator", &[], "Operator"),
        ];
        // itertools returns the first item that matches the predicate in unique_by, so custom
        // mappings go first to favor the user configured mappings over the default ones.
        let mut mappings = res.semantic_token_mappings;
        mappings.append(&mut default_mappings);
        let semantic_token_mappings = mappings
            .into_iter()
            .unique_by(|i| format!("{}.{}", i.modifiers.join("."), i.name))
            .collect();

        let loaded_fzf = vim.eval::<_, i64>("get(g:, 'loaded_fzf')")? == 1;
        let selection_ui = match res.selection_ui {
            Some(s) => SelectionUI::from_str(&s)?,
            None if loaded_fzf => SelectionUI::Funcref,
            None => SelectionUI::default(),
        };

        let diagnostics_list = match res.diagnostics_list {
            Some(s) => DiagnosticsList::from_str(&s)?,
            None => DiagnosticsList::Disabled,
        };

        let hover_preview = match res.hover_preview {
            Some(s) => HoverPreviewOption::from_str(&s)?,
            None => HoverPreviewOption::Auto,
        };

        let mut diagnostics_display = DiagnosticsDisplay::default();
        res.diagnostics_display.into_iter().for_each(|(k, v)| {
            diagnostics_display.insert(k, v);
        });

        Ok(Config {
            auto_start: res.auto_start == 1,
            server_commands: res.server_commands,
            selection_ui,
            trace: trace(&res.trace.unwrap_or_else(|| "off".to_string()))?,
            settings_path: res.settings_path,
            load_settings: res.load_settings == 1,
            root_markers: res.root_markers,
            change_throttle: res
                .change_throttle
                .map(|t| Duration::from_millis((t * 1000.0) as u64)),
            wait_output_timeout: Duration::from_millis(
                (res.wait_output_timeout.unwrap_or(10.0) * 1000.0) as u64,
            ),
            diagnostics_enable: res.diagnostics_enable == 1,
            diagnostics_list,
            diagnostics_display,
            code_lens_display: res.code_lens_display.unwrap_or_default(),
            window_log_message_level: message_type(&res.window_log_message_level)?,
            hover_preview,
            completion_prefer_text_edit: res.completion_prefer_text_edit == 1,
            is_nvim: res.is_nvim == 1,
            server_stderr: res.server_stderr,
            diagnostics_signs_max: res.diagnostics_signs_max,
            diagnostics_max_severity: diagnostics_severity(&res.diagnostics_max_severity)?,
            diagnostics_ignore_sources: res.diagnostics_ignore_sources,
            document_highlight_display: res.document_highlight_display.unwrap_or_default(),
            selection_ui_auto_open: res.selection_ui_auto_open == 1,
            use_virtual_text: res.use_virtual_text,
            echo_project_root: res.echo_project_root == 1,
            apply_completion_text_edits: res.apply_completion_text_edits == 1,
            preferred_markup_kind: res.preferred_markup_kind,
            hide_virtual_texts_on_insert: res.hide_virtual_texts_on_insert == 1,
            enable_extensions: res.enable_extensions,
            restart_on_crash: res.restart_on_crash == 1,
            max_restart_retries: res.max_restart_retries,
            semantic_token_mappings,
            semantic_highlighting_enabled: res.semantic_highlighting_enabled == 1,
        })
    }
}

fn trace(s: &str) -> Result<TraceOption> {
    match s.to_ascii_uppercase().as_str() {
        "OFF" => Ok(TraceOption::Off),
        "MESSAGES" => Ok(TraceOption::Messages),
        "VERBOSE" => Ok(TraceOption::Verbose),
        _ => Err(anyhow!("Invalid option for LanguageClient_trace: {}", s)),
    }
}

fn message_type(s: &str) -> Result<MessageType> {
    match s.to_ascii_uppercase().as_str() {
        "ERROR" => Ok(MessageType::Error),
        "WARNING" => Ok(MessageType::Warning),
        "INFO" => Ok(MessageType::Info),
        "LOG" => Ok(MessageType::Log),
        _ => Err(anyhow!(
            "Invalid option for LanguageClient_windowLogMessageLevel: {}",
            s,
        )),
    }
}

fn diagnostics_severity(s: &str) -> Result<DiagnosticSeverity> {
    match s.to_ascii_uppercase().as_str() {
        "ERROR" => Ok(DiagnosticSeverity::Error),
        "WARNING" => Ok(DiagnosticSeverity::Warning),
        "INFORMATION" => Ok(DiagnosticSeverity::Information),
        "HINT" => Ok(DiagnosticSeverity::Hint),
        _ => Err(anyhow!(
            "Invalid option for LanguageClient_diagnosticsMaxSeverity: {}",
            s
        )),
    }
}
