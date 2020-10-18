use crate::types::{
    DiagnosticsDisplay, DiagnosticsList, DocumentHighlightDisplay, HoverPreviewOption, RootMarkers,
    SelectionUI, UseVirtualText,
};
use lsp_types::{DiagnosticSeverity, MarkupKind, MessageType, TraceOption};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::{path::PathBuf, str::FromStr, time::Duration};
use vim_eval_derive::*;

#[derive(Debug, Deserialize, VimEval)]
pub struct Config {
    #[vim_eval(default = "1")]
    #[serde(deserialize_with = "bool_from_int")]
    pub auto_start: bool,
    #[vim_eval(eval_with = "s:GetVar('LanguageClient_serverCommands', {})")]
    pub server_commands: HashMap<String, Vec<String>>,
    #[vim_eval(eval_with = "s:getSelectionUI()")]
    #[serde(deserialize_with = "from_upper_str")]
    pub selection_ui: SelectionUI,
    #[serde(deserialize_with = "trace_option_from_str")]
    pub trace: TraceOption,
    #[vim_eval(
        eval_with = "map(s:ToList(get(g:, 'LanguageClient_settingsPath', '.vim/settings.json')), 'expand(v:val)')"
    )]
    pub settings_path: Vec<String>,
    #[vim_eval(default = "1")]
    #[serde(deserialize_with = "bool_from_int")]
    pub load_settings: bool,
    pub root_markers: Option<RootMarkers>,
    #[serde(deserialize_with = "optional_duration_from_int")]
    pub change_throttle: Option<Duration>,
    #[vim_eval(default = "10")]
    #[serde(deserialize_with = "duration_from_int")]
    pub wait_output_timeout: Duration,
    #[vim_eval(default = "1")]
    #[serde(deserialize_with = "bool_from_int")]
    pub diagnostics_enable: bool,
    #[vim_eval(default = "'Quickfix'")]
    #[serde(deserialize_with = "from_upper_str")]
    pub diagnostics_list: DiagnosticsList,
    #[vim_eval(default = "{}")]
    pub diagnostics_display: HashMap<u64, DiagnosticsDisplay>,
    #[vim_eval(default = "'Warning'")]
    #[serde(deserialize_with = "message_type_from_str")]
    pub window_log_message_level: MessageType,
    #[vim_eval(default = "'Auto'")]
    #[serde(deserialize_with = "from_upper_str")]
    pub hover_preview: HoverPreviewOption,
    #[vim_eval(default = "0")]
    #[serde(deserialize_with = "bool_from_int")]
    pub completion_prefer_text_edit: bool,
    #[vim_eval(eval_with = "has('nvim')")]
    #[serde(deserialize_with = "bool_from_int")]
    pub is_nvim: bool,
    pub logging_file: Option<PathBuf>,
    #[vim_eval(default = "'WARN'")]
    #[serde(deserialize_with = "from_upper_str")]
    pub logging_level: log::LevelFilter,
    pub server_stderr: Option<String>,
    pub diagnostics_signs_max: Option<usize>,
    #[vim_eval(default = "'Hint'")]
    #[serde(deserialize_with = "diagnostic_severity_from_str")]
    pub diagnostics_max_severity: DiagnosticSeverity,
    #[vim_eval(default = "[]")]
    pub diagnostics_ignore_sources: Vec<String>,
    #[vim_eval(default = "{}")]
    pub document_highlight_display: HashMap<u64, DocumentHighlightDisplay>,
    #[vim_eval(default = "1")]
    #[serde(deserialize_with = "bool_from_int")]
    pub selection_ui_auto_open: bool,
    #[vim_eval(eval_with = "s:useVirtualText()")]
    pub use_virtual_text: UseVirtualText,
    #[vim_eval(default = "1")]
    #[serde(deserialize_with = "bool_from_int")]
    pub echo_project_root: bool,
    #[vim_eval(default = "{}")]
    pub semantic_highlight_maps: HashMap<String, HashMap<String, String>>,
    #[vim_eval(default = "':'")]
    pub semantic_scope_separator: String,
    #[vim_eval(default = "1")]
    #[serde(deserialize_with = "bool_from_int")]
    pub apply_completion_text_edits: bool,
    pub preferred_markup_kind: Option<Vec<MarkupKind>>,
    #[vim_eval(default = "0")]
    #[serde(deserialize_with = "bool_from_int")]
    pub hide_virtual_texts_on_insert: bool,
    pub enable_extensions: Option<HashMap<String, bool>>,
    #[vim_eval(default = "'Comment'")]
    pub code_lens_highlight_group: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_commands: HashMap::new(),
            semantic_highlight_maps: HashMap::new(),
            semantic_scope_separator: ":".into(),
            auto_start: true,
            selection_ui: SelectionUI::LocationList,
            selection_ui_auto_open: true,
            trace: TraceOption::default(),
            diagnostics_enable: true,
            diagnostics_list: DiagnosticsList::Quickfix,
            diagnostics_display: DiagnosticsDisplay::default(),
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
            code_lens_highlight_group: "Comment".into(),
            is_nvim: false,
            logging_file: None,
            logging_level: log::LevelFilter::Off,
        }
    }
}

fn optional_duration_from_int<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    match <Option<u64>>::deserialize(deserializer)? {
        None => Ok(None),
        Some(t) => Ok(Some(Duration::from_millis(t * 1000))),
    }
}

fn duration_from_int<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let t = <Option<u64>>::deserialize(deserializer)?.unwrap_or_default();
    Ok(Duration::from_millis(t * 1000))
}

fn bool_from_int<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match u8::deserialize(deserializer)? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(serde::de::Error::invalid_value(
            serde::de::Unexpected::Unsigned(other as u64),
            &"zero or one",
        )),
    }
}

fn message_type_from_str<'de, D>(deserializer: D) -> Result<MessageType, D::Error>
where
    D: Deserializer<'de>,
{
    match <Option<String>>::deserialize(deserializer)?
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "ERROR" => Ok(MessageType::Error),
        "WARNING" => Ok(MessageType::Warning),
        "INFO" => Ok(MessageType::Info),
        "LOG" => Ok(MessageType::Log),
        "" => Ok(MessageType::Warning),
        other => Err(serde::de::Error::invalid_value(
            serde::de::Unexpected::Str(other.as_ref()),
            &"error, warning, info or log",
        )),
    }
}

fn trace_option_from_str<'de, D>(deserializer: D) -> Result<TraceOption, D::Error>
where
    D: Deserializer<'de>,
{
    match <Option<String>>::deserialize(deserializer)?
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "OFF" => Ok(TraceOption::Off),
        "MESSAGES" => Ok(TraceOption::Messages),
        "VERBOSE" => Ok(TraceOption::Verbose),
        "" => Ok(TraceOption::default()),
        other => Err(serde::de::Error::invalid_value(
            serde::de::Unexpected::Str(other.as_ref()),
            &"off, messages or verbose",
        )),
    }
}

fn diagnostic_severity_from_str<'de, D>(deserializer: D) -> Result<DiagnosticSeverity, D::Error>
where
    D: Deserializer<'de>,
{
    match <Option<String>>::deserialize(deserializer)?
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "ERROR" => Ok(DiagnosticSeverity::Error),
        "WARNING" => Ok(DiagnosticSeverity::Warning),
        "INFORMATION" => Ok(DiagnosticSeverity::Information),
        "HINT" => Ok(DiagnosticSeverity::Hint),
        "" => Ok(DiagnosticSeverity::Hint),
        other => Err(serde::de::Error::invalid_value(
            serde::de::Unexpected::Str(other.as_ref()),
            &"off, messages or verbose",
        )),
    }
}

fn from_upper_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    T::Err: std::fmt::Display,
    D: Deserializer<'de>,
{
    let upper = String::deserialize(deserializer)?.to_ascii_uppercase();
    T::from_str(&upper).map_err(serde::de::Error::custom)
}

// fn selection_ui_from_str<'de, D>(deserializer: D) -> Result<SelectionUI, D::Error>
// where
//     D: Deserializer<'de>,
// {
//     match <Option<String>>::deserialize(deserializer)?
//         .unwrap_or_default()
//         .to_ascii_uppercase()
//         .as_str()
//     {
//         "" => Ok(SelectionUI::default()),
//         s => Ok(SelectionUI::from_str(s).map_err(|_| {
//             serde::de::Error::invalid_value(serde::de::Unexpected::Str(s), &"always, auto or never")
//         })?),
//     }
// }
