use crate::config::{Config, ServerCommand};
use crate::extensions::java;
use crate::language_client::LanguageClient;
use crate::sign::Sign;
use crate::vim::{try_get, Mode};
use crate::{
    rpcclient::RpcClient,
    types::*,
    utils::{
        apply_text_edits, code_action_kind_as_str, convert_to_vim_str, decode_parameter_label,
        escape_single_quote, expand_json_path, get_default_initialization_options, get_root_path,
        vim_cmd_args_to_value, Canonicalize, Combine, ToUrl,
    },
    viewport,
    watcher::FSWatch,
};
use crate::{viewport::Viewport, vim::Highlight};
use anyhow::{anyhow, Context, Error, Result};
use glob::glob;
use itertools::Itertools;
use jsonrpc_core::Value;
use log::{debug, error, info, warn};
use lsp_types::{
    notification::Notification, request::Request, ApplyWorkspaceEditParams,
    ApplyWorkspaceEditResponse, ClientCapabilities, ClientInfo, CodeAction, CodeActionCapability,
    CodeActionContext, CodeActionKind, CodeActionKindLiteralSupport, CodeActionLiteralSupport,
    CodeActionOrCommand, CodeActionParams, CodeActionResponse, CodeLens, Command,
    CompletionCapability, CompletionItem, CompletionItemCapability, CompletionResponse,
    CompletionTextEdit, Diagnostic, DiagnosticSeverity, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidChangeWatchedFilesRegistrationOptions, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentChangeOperation, DocumentChanges,
    DocumentFormattingParams, DocumentHighlight, DocumentHighlightKind,
    DocumentRangeFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, Documentation,
    ExecuteCommandParams, FormattingOptions, GenericCapability, GotoCapability,
    GotoDefinitionResponse, Hover, HoverCapability, InitializeParams, InitializeResult,
    InitializedParams, Location, LogMessageParams, MessageType, NumberOrString,
    ParameterInformation, ParameterInformationSettings, PartialResultParams, Position,
    ProgressParams, ProgressParamsValue, PublishDiagnosticsClientCapabilities,
    PublishDiagnosticsParams, Range, ReferenceContext, RegistrationParams, RenameParams,
    ResourceOp, SemanticHighlightingClientCapability, SemanticHighlightingParams,
    ShowMessageParams, ShowMessageRequestParams, SignatureHelp, SignatureHelpCapability,
    SignatureInformationSettings, SymbolInformation, TextDocumentClientCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TextEdit, UnregistrationParams, VersionedTextDocumentIdentifier,
    WorkDoneProgress, WorkDoneProgressParams, WorkspaceClientCapabilities, WorkspaceEdit,
    WorkspaceSymbolParams,
};
use maplit::hashmap;
use serde::de::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    fs::{read_to_string, File},
    io::{BufRead, BufReader, BufWriter},
    net::TcpStream,
    path::Path,
    process::Stdio,
    sync::{mpsc, Arc, MutexGuard},
    thread,
    time::{Duration, Instant},
};

#[derive(PartialEq)]
pub enum Direction {
    Next,
    Previous,
}

impl LanguageClient {
    pub fn get_client(&self, language_id: &LanguageId) -> Result<Arc<RpcClient>> {
        self.get_state(|state| state.clients.get(language_id).cloned())?
            .ok_or_else(|| {
                LCError::ServerNotRunning {
                    language_id: language_id.clone().unwrap_or_default(),
                }
                .into()
            })
    }

    pub fn loop_call(&self, rx: &crossbeam::channel::Receiver<Call>) -> Result<()> {
        for call in rx.iter() {
            let language_client = self.clone();
            thread::spawn(move || {
                if let Err(err) = language_client.handle_call(call) {
                    error!("Error handling request:\n{:?}", err);
                }
            });
        }

        Ok(())
    }

    /////// Utils ///////
    #[tracing::instrument(level = "info", skip(self))]
    fn sync_settings(&self) -> Result<()> {
        let mut config = Config::parse(self.vim()?)?;
        self.update_state(|state| {
            state
                .logger
                .update_settings(config.logging_level.clone(), config.logging_file.clone())
        })?;

        let semantic_highlight_language_ids: Vec<String> =
            config.semantic_highlight_maps.keys().cloned().collect();

        // merge defaults with user provided config
        let mut diagnostics_display = self.get_config(|c| c.diagnostics_display.clone())?;
        diagnostics_display.extend(config.diagnostics_display);
        config.diagnostics_display = diagnostics_display;

        // merge defaults with user provided config
        let mut document_highlight_display =
            self.get_config(|c| c.document_highlight_display.clone())?;
        document_highlight_display.extend(config.document_highlight_display);
        config.document_highlight_display = document_highlight_display;

        self.update_config(|c| *c = config)?;

        self.update_state(|state| {
            state.semantic_scope_to_hl_group_table.clear();

            Ok(())
        })?;

        for language_id in semantic_highlight_language_ids {
            self.update_semantic_highlight_tables(&language_id)?;
        }

        Ok(())
    }

    fn get_workspace_settings(&self, root: &str) -> Result<Value> {
        if !self.get_config(|c| c.load_settings)? {
            return Ok(Value::Null);
        }

        let mut res = Value::Null;
        let mut last_err = None;
        let mut at_least_one_success = false;
        for orig_path in self.get_config(|c| c.settings_path.clone())? {
            let path = Path::new(root).join(orig_path);
            let buffer = read_to_string(&path)
                .with_context(|| format!("Failed to read file ({})", path.to_string_lossy()));
            let buffer = match buffer {
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
                Ok(x) => x,
            };
            let value = serde_json::from_str(&buffer);
            let value = match value {
                Err(e) => {
                    last_err = Some(e.into());
                    continue;
                }
                Ok(x) => x,
            };
            let value = expand_json_path(value);
            json_patch::merge(&mut res, &value);
            at_least_one_success = true;
        }

        match last_err {
            // no file was read and an error happened
            Some(e) if !at_least_one_success => Err(e),
            _ => Ok(res),
        }
    }

    fn define_signs(&self) -> Result<()> {
        let mut cmds = vec![];
        let diagnostics_display = self.get_config(|c| c.diagnostics_display.clone())?;
        for entry in diagnostics_display.values() {
            cmds.push(format!(
                "sign define LanguageClient{} text={} texthl={}",
                entry.name, entry.sign_text, entry.sign_texthl,
            ));
        }

        self.vim()?.command(cmds)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn apply_workspace_edit(&self, edit: &WorkspaceEdit) -> Result<()> {
        let mut filename = self.vim()?.get_filename(&Value::Null)?;
        let mut position = self.vim()?.get_position(&Value::Null)?;
        if let Some(ref changes) = edit.document_changes {
            match changes {
                DocumentChanges::Edits(ref changes) => {
                    for e in changes {
                        position = self.apply_text_edits(
                            &e.text_document.uri.filepath()?,
                            &e.edits,
                            position,
                        )?;
                    }
                }
                DocumentChanges::Operations(ref ops) => {
                    for op in ops {
                        match op {
                            DocumentChangeOperation::Edit(ref e) => {
                                position = self.apply_text_edits(
                                    &e.text_document.uri.filepath()?,
                                    &e.edits,
                                    position,
                                )?
                            }
                            DocumentChangeOperation::Op(ref rop) => match rop {
                                ResourceOp::Create(file) => {
                                    filename = file.uri.filepath()?.to_string_lossy().into_owned();
                                    position = Position::default();
                                }
                                ResourceOp::Rename(_file) => {
                                    return Err(anyhow!("file renaming not yet supported."));
                                }
                                ResourceOp::Delete(_file) => {
                                    return Err(anyhow!("file deletion not yet supported."));
                                }
                            },
                        }
                    }
                }
            }
        } else if let Some(ref changes) = edit.changes {
            for (uri, edits) in changes {
                position = self.apply_text_edits(&uri.filepath()?, edits, position)?;
            }
        }
        self.edit(&None, &filename)?;
        self.vim()?
            .cursor(position.line + 1, position.character + 1)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_document_highlight(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(&Value::Null)?;
        let language_id = self.vim()?.get_language_id(&filename, &Value::Null)?;
        let position = self.vim()?.get_position(&Value::Null)?;

        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::DocumentHighlightRequest::METHOD,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position,
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let document_highlight = <Option<Vec<DocumentHighlight>>>::deserialize(&result)?;
        if let Some(document_highlight) = document_highlight {
            let document_highlight_display =
                self.get_config(|c| c.document_highlight_display.clone())?;
            let highlights = document_highlight
                .into_iter()
                .map(|DocumentHighlight { range, kind }| {
                    Ok(Highlight {
                        line: range.start.line,
                        character_start: range.start.character,
                        character_end: range.end.character,
                        group: document_highlight_display
                            .get(
                                &kind
                                    .unwrap_or(DocumentHighlightKind::Text)
                                    .to_int()
                                    .unwrap(),
                            )
                            .ok_or_else(|| anyhow!("Failed to get display"))?
                            .texthl
                            .clone(),
                        text: String::new(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            self.vim()?
                .set_highlights(&highlights, "__LCN_DOCUMENT_HIGHLIGHT__")?;
        }

        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn clear_document_highlight(&self, _params: &Value) -> Result<()> {
        self.vim()?.clear_highlights("__LCN_DOCUMENT_HIGHLIGHT__")
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn apply_text_edits<P: AsRef<Path> + std::fmt::Debug>(
        &self,
        path: P,
        edits: &[TextEdit],
        position: Position,
    ) -> Result<Position> {
        if edits.is_empty() {
            return Ok(position);
        }

        let mut edits = edits.to_vec();

        // Edits must be applied from bottom to top, so that earlier edits will not interfere
        // with the positioning of later edits. Edits that start with the same position must be
        // applied in reverse order, so that multiple inserts will have their text appear in the
        // same order the server sent it, and so that a delete/replace (according to the LSP spec,
        // there can only be one per start position and it must be after the inserts) will work on
        // the original document, not on the just-inserted text.
        edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        edits.reverse();

        self.edit(&None, path)?;

        let mut lines: Vec<String> = self.vim()?.rpcclient.call("getline", json!([1, '$']))?;
        let lines_len_prev = lines.len();
        let fixendofline = self.vim()?.eval::<_, u8>("&fixendofline")? == 1;
        if lines.last().map(String::is_empty) == Some(false) && fixendofline {
            lines.push("".to_owned());
        }

        let (mut lines, position) = apply_text_edits(&lines, &edits, &position)?;

        if lines.last().map(String::is_empty) == Some(true) && fixendofline {
            lines.pop();
        }
        if lines.len() < lines_len_prev {
            self.vim()?
                .command(format!("{},{}d", lines.len() + 1, lines_len_prev))?;
        }
        self.vim()?.rpcclient.notify("setline", json!([1, lines]))?;
        Ok(position)
    }

    // moves the cursor to the next or previous diagnostic, depending on the value of direction.
    pub fn cycle_diagnostics(&self, params: &Value, direction: Direction) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let pos = self.vim()?.get_position(params)?;
        let mut diagnostics = self.get_state(|state| state.diagnostics.clone())?;
        if let Some(diagnostics) = diagnostics.get_mut(&filename) {
            if direction == Direction::Next {
                diagnostics.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
            } else {
                diagnostics.sort_by_key(|edit| {
                    (
                        -(edit.range.start.line as i64),
                        -(edit.range.start.character as i64),
                    )
                });
            }

            let (line, col) = (pos.line, pos.character);
            if let Some((_, diagnostic)) = diagnostics.iter_mut().find_position(|it| {
                let start = it.range.start;
                if direction == Direction::Next {
                    start.line > line || (start.line == line && start.character > col)
                } else {
                    start.line < line || (start.line == line && start.character < col)
                }
            }) {
                let line = diagnostic.range.start.line + 1;
                let col = diagnostic.range.start.character + 1;
                self.vim()?.cursor(line, col)?;
            } else {
                self.vim()?.echomsg("No diagnostics found")?;
            }
        } else {
            self.vim()?.echomsg("No diagnostics found")?;
        }

        Ok(())
    }

    fn update_quickfixlist(&self) -> Result<()> {
        let diagnostics = self.get_state(|state| state.diagnostics.clone())?;
        let qflist: Vec<_> = diagnostics
            .iter()
            .flat_map(|(filename, diagnostics)| {
                diagnostics
                    .iter()
                    .map(|dn| QuickfixEntry {
                        filename: filename.to_owned(),
                        lnum: dn.range.start.line + 1,
                        col: Some(dn.range.start.character + 1),
                        nr: dn.code.clone().map(|ns| ns.to_string()),
                        text: Some(dn.message.to_owned()),
                        typ: dn.severity.map(|sev| sev.to_quickfix_entry_type()),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let title = "[LC]: diagnostics";

        match self.get_config(|c| c.diagnostics_list)? {
            DiagnosticsList::Quickfix => {
                self.vim()?.setqflist(&qflist, "r", title)?;
            }
            DiagnosticsList::Location => {
                self.vim()?.setloclist(&qflist, "r", title)?;
            }
            DiagnosticsList::Disabled => {}
        }

        Ok(())
    }

    fn process_diagnostics(&self, filename: &str, diagnostics: &[Diagnostic]) -> Result<()> {
        if !self.get_state(|state| state.text_documents.contains_key(filename))? {
            return Ok(());
        }

        let text = self.get_state(|state| {
            state
                .text_documents
                .get(filename)
                .map(|d| d.text.clone())
                .unwrap_or_default()
        })?;
        let lines: Vec<_> = text.lines().map(ToOwned::to_owned).collect();

        // Line diagnostics.
        let mut line_diagnostics = HashMap::new();
        for entry in diagnostics {
            let line = entry.range.start.line;
            let mut msg = String::new();
            if let Some(severity) = entry.severity {
                msg += &format!("[{:?}] ", severity);
            }
            if let Some(ref code) = entry.code {
                let s = code.to_string();
                if !s.is_empty() {
                    msg += &format!("[{}] ", s);
                }
            }
            msg += &entry.message;
            line_diagnostics.insert((filename.to_owned(), line), msg);
        }
        self.update_state(|state| {
            state
                .line_diagnostics
                .retain(|&(ref f, _), _| f != filename);
            state.line_diagnostics.extend(line_diagnostics);
            Ok(())
        })?;

        // Highlight.
        let diagnostics_display = self.get_config(|c| c.diagnostics_display.clone())?;

        let mut highlights = vec![];
        for dn in diagnostics {
            let line = dn.range.start.line;
            let character_start = dn.range.start.character;
            let character_end = dn.range.end.character;

            let severity = dn.severity.unwrap_or(DiagnosticSeverity::Hint);
            let group = diagnostics_display
                .get(&severity.to_int()?)
                .ok_or_else(|| anyhow!("Failed to get display"))?
                .texthl
                .clone();
            // TODO: handle multi-line range.
            let text = lines
                .get(line as usize)
                .and_then(|l| l.get((character_start as usize)..(character_end as usize)))
                .map(ToOwned::to_owned)
                .unwrap_or_default();

            highlights.push(Highlight {
                line,
                character_start,
                character_end,
                group,
                text,
            });
        }
        // dedup?
        self.update_state(|state| {
            state.highlights.insert(filename.to_owned(), highlights);
            Ok(())
        })?;

        if !self.get_config(|c| c.is_nvim)? {
            // Clear old highlights.
            let ids = self.get_state(|state| state.highlight_match_ids.clone())?;
            self.vim()?
                .rpcclient
                .notify("s:MatchDelete", json!([ids]))?;

            // Group diagnostics by severity so we can highlight them
            // in a single call.
            let mut match_groups: HashMap<_, Vec<_>> = HashMap::new();

            for dn in diagnostics {
                let severity = dn.severity.unwrap_or(DiagnosticSeverity::Hint).to_int()?;
                match_groups
                    .entry(severity)
                    .or_insert_with(Vec::new)
                    .push(dn);
            }

            let mut new_match_ids = Vec::new();

            for (severity, dns) in match_groups {
                let hl_group = diagnostics_display
                    .get(&severity)
                    .ok_or_else(|| anyhow!("Failed to get display"))?
                    .texthl
                    .clone();
                let ranges: Vec<Vec<_>> = dns
                    .iter()
                    .flat_map(|dn| {
                        if dn.range.start.line == dn.range.end.line {
                            let length = dn.range.end.character - dn.range.start.character;
                            // Vim line numbers are 1 off
                            // `matchaddpos` expects an array of [line, col, length]
                            // for each match.
                            vec![vec![
                                dn.range.start.line + 1,
                                dn.range.start.character + 1,
                                length,
                            ]]
                        } else {
                            let mut middle_lines: Vec<_> = (dn.range.start.line + 1
                                ..dn.range.end.line)
                                .map(|l| vec![l + 1])
                                .collect();
                            let start_line = vec![
                                dn.range.start.line + 1,
                                dn.range.start.character + 1,
                                999_999, //Clear to the end of the line
                            ];
                            let end_line =
                                vec![dn.range.end.line + 1, 1, dn.range.end.character + 1];
                            middle_lines.push(start_line);
                            // For a multi-ringe range ending at the exact start of the last line,
                            // don't highlight the first character of the last line.
                            if dn.range.end.character > 0 {
                                middle_lines.push(end_line);
                            }
                            middle_lines
                        }
                    })
                    .collect();

                let match_id = self
                    .vim()?
                    .rpcclient
                    .call("matchaddpos", json!([hl_group, ranges]))?;
                new_match_ids.push(match_id);
            }
            self.update_state(|state| {
                state.highlight_match_ids = new_match_ids;
                Ok(())
            })?;
        }

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn register_cm_source(&self, language_id: &str, result: &Value) -> Result<()> {
        let exists_cm_register: u64 = self.vim()?.eval("exists('g:cm_matcher')")?;
        if exists_cm_register == 0 {
            return Ok(());
        }

        let result = InitializeResult::deserialize(result)?;
        if result.capabilities.completion_provider.is_none() {
            return Ok(());
        }

        let trigger_patterns = result
            .capabilities
            .completion_provider
            .map(|opt| {
                let strings: Vec<_> = opt
                    .trigger_characters
                    .unwrap_or_default()
                    .iter()
                    .map(|c| regex::escape(c))
                    .collect();
                strings
            })
            .unwrap_or_default();

        self.vim()?.rpcclient.notify(
            "cm#register_source",
            json!([{
                "name": format!("LanguageClient_{}", language_id),
                "priority": 9,
                "scopes": [language_id],
                "cm_refresh_patterns": trigger_patterns,
                "abbreviation": "LC",
                "cm_refresh": REQUEST_NCM_REFRESH,
            }]),
        )?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn register_ncm2_source(&self, language_id: &str, result: &Value) -> Result<()> {
        let exists_ncm2: u64 = self.vim()?.eval("exists('g:ncm2_loaded')")?;
        if exists_ncm2 == 0 {
            return Ok(());
        }

        let result = InitializeResult::deserialize(result)?;
        if result.capabilities.completion_provider.is_none() {
            return Ok(());
        }

        let trigger_patterns = result
            .capabilities
            .completion_provider
            .map(|opt| {
                let strings: Vec<_> = opt
                    .trigger_characters
                    .unwrap_or_default()
                    .iter()
                    .map(|c| regex::escape(c))
                    .collect();
                strings
            })
            .unwrap_or_default();

        self.vim()?.rpcclient.notify(
            "ncm2#register_source",
            json!([{
                "name": format!("LanguageClient_{}", language_id),
                "priority": 9,
                "scope": [language_id],
                "complete_pattern": trigger_patterns,
                "mark": "LC",
                "on_complete": REQUEST_NCM2_ON_COMPLETE,
            }]),
        )?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn parse_semantic_scopes(&self, language_id: &str, result: &Value) -> Result<()> {
        let result = InitializeResult::deserialize(result)?;

        if let Some(capability) = result.capabilities.semantic_highlighting {
            self.update_state(|state| {
                state
                    .semantic_scopes
                    .insert(language_id.into(), capability.scopes.unwrap_or_default());
                Ok(())
            })?;
        }

        Ok(())
    }

    /// Build the Semantic Highlight Lookup Table of
    ///
    /// ScopeIndex -> Option<HighlightGroup>
    #[tracing::instrument(level = "info", skip(self))]
    fn update_semantic_highlight_tables(&self, language_id: &str) -> Result<()> {
        let opt_scopes = self.get_state(|state| state.semantic_scopes.get(language_id).cloned())?;
        let opt_hl_map =
            self.get_config(|c| c.semantic_highlight_maps.get(language_id).cloned())?;
        let scope_separator = self.get_config(|c| c.semantic_scope_separator.clone())?;
        if let (Some(semantic_scopes), Some(shm)) = (opt_scopes, opt_hl_map) {
            let mut table: Vec<Option<String>> = Vec::new();

            for scope_list in semantic_scopes {
                // Combine all scopes ["scopeA", "scopeB", ...] -> "scopeA:scopeB:..."
                let scope_str = scope_list.iter().join(&scope_separator);

                let mut matched = false;
                for (scope_regex, hl_group) in &shm {
                    let match_expr = format!(
                        "({} =~ {})",
                        convert_to_vim_str(&scope_str),
                        convert_to_vim_str(scope_regex)
                    );

                    let matches: i32 = self.vim()?.eval(match_expr)?;

                    if matches == 1 {
                        table.push(Some(hl_group.clone()));
                        matched = true;
                        break;
                    }
                }

                if !matched {
                    table.push(None);
                }
            }

            self.update_state(|state| {
                state
                    .semantic_scope_to_hl_group_table
                    .insert(language_id.into(), table);
                Ok(())
            })?;
        } else {
            self.update_state(|state| {
                state.semantic_scope_to_hl_group_table.remove(language_id);
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn get_line(&self, path: impl AsRef<Path>, line: u64) -> Result<String> {
        let value: Value = self.vim()?.rpcclient.call(
            "getbufline",
            json!([path.as_ref().to_string_lossy(), line + 1]),
        )?;
        let mut texts = <Vec<String>>::deserialize(value)?;
        let mut text = texts.pop().unwrap_or_default();

        if text.is_empty() {
            let reader = BufReader::new(File::open(path)?);
            text = reader
                .lines()
                .nth(line.to_usize()?)
                .ok_or_else(|| anyhow!("Failed to get line! line: {}", line))??;
        }

        Ok(text.trim().into())
    }

    fn try_handle_command_by_client(&self, cmd: &Command) -> Result<bool> {
        let filetype: String = self.vim()?.eval("&filetype")?;
        if !self.extensions_enabled(&filetype)? {
            return Ok(false);
        }

        let capabilities = self.get_state(|state| state.capabilities.get(&filetype).cloned())?;
        let server_name = capabilities
            .unwrap_or_default()
            .server_info
            .unwrap_or_default()
            .name;

        match server_name.as_str() {
            "gopls" => self.handle_gopls_command(cmd),
            "rust-analyzer" => self.handle_rust_analyzer_command(cmd),
            _ => match cmd.command.as_str() {
                // not sure which name java's language server advertises
                java::command::APPLY_WORKSPACE_EDIT => self.handle_java_command(cmd),
                _ => Ok(false),
            },
        }
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn cleanup(&self, language_id: &str) -> Result<()> {
        let root = self.get_state(|state| {
            state
                .roots
                .get(language_id)
                .cloned()
                .ok_or_else(|| anyhow!("No project root found! languageId: {}", language_id))
        })??;

        let mut filenames = vec![];
        self.update_state(|state| {
            for (f, diag_list) in state.diagnostics.iter_mut() {
                if f.starts_with(&root) {
                    filenames.push(f.clone());
                    diag_list.clear();
                }
            }
            Ok(())
        })?;

        for f in filenames {
            if let Ok(bufnr) = self.vim()?.eval::<_, Bufnr>(format!("bufnr('{}')", f)) {
                // Some Language Server diagnoses non-opened buffer, so we must check if buffer exists.
                if bufnr > 0 {
                    self.vim()?.rpcclient.notify(
                        "setbufvar",
                        json!([f, VIM_STATUS_LINE_DIAGNOSTICS_COUNTS, {}]),
                    )?;
                }
            }
            self.process_diagnostics(&f, &[])?;
        }
        self.handle_cursor_moved(&Value::Null, true)?;

        self.update_state(|state| {
            state.clients.remove(&Some(language_id.into()));
            state.last_cursor_line = 0;
            state.text_documents.retain(|f, _| !f.starts_with(&root));
            state.roots.remove(language_id);
            Ok(())
        })?;
        self.update_quickfixlist()?;

        self.vim()?.command(vec![
            format!("let {}=0", VIM_SERVER_STATUS),
            format!("let {}=''", VIM_SERVER_STATUS_MESSAGE),
        ])?;
        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageClientStopped")?;

        Ok(())
    }

    fn preview<D>(&self, to_display: &D, bufname: &str) -> Result<()>
    where
        D: ToDisplay + ?Sized,
    {
        let filetype = &to_display.vim_filetype();
        let lines = to_display.to_display();

        self.vim()?
            .rpcclient
            .notify("s:OpenHoverPreview", json!([bufname, lines, filetype]))?;

        Ok(())
    }

    fn edit(&self, goto_cmd: &Option<String>, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().to_string_lossy();
        if path.starts_with("jdt://") {
            self.java_class_file_contents(&json!({ "gotoCmd": goto_cmd, "uri": path }))?;
            Ok(())
        } else {
            self.vim()?.edit(&goto_cmd, path.into_owned())
        }
    }

    /////// LSP ///////

    #[tracing::instrument(level = "info", skip(self))]
    fn initialize(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let has_snippet_support: i8 = try_get("hasSnippetSupport", params)?
            .map_or_else(|| self.vim()?.eval("s:hasSnippetSupport()"), Ok)?;
        let has_snippet_support = has_snippet_support > 0;
        let root =
            self.get_state(|state| state.roots.get(&language_id).cloned().unwrap_or_default())?;

        let trace = self.get_config(|c| c.trace)?;
        let preferred_markup_kind = self.get_config(|c| c.preferred_markup_kind.clone())?;
        let command = self.get_config(|c| c.server_commands.get(&language_id).cloned())?;
        if command.is_none() {
            return Err(anyhow!(
                "No server command found for language {}",
                language_id
            ));
        }
        let command = command.unwrap();

        let settings = self.get_workspace_settings(&root).unwrap_or_default();
        // warn the user that they are using a deprecated workspace settings
        // file format and direct them to the documentation about the new one
        if settings.pointer("/initializationOptions").is_some() {
            let _ = self.vim()?.echoerr("You seem to be using an incorrect workspace settings format for LanguageClient-neovim, to learn more about this error see `:help g:LanguageClient_settingsPath`");
        }

        let initialization_options = merged_initialization_options(&command, &settings)?;

        let result: Value = self.get_client(&Some(language_id.clone()))?.call(
            lsp_types::request::Initialize::METHOD,
            #[allow(deprecated)]
            InitializeParams {
                client_info: Some(ClientInfo {
                    name: "LanguageClient-neovim".into(),
                    version: Some(self.version()),
                }),
                process_id: Some(u64::from(std::process::id())),
                /* deprecated in lsp types, but can't initialize without it */
                root_path: Some(root.clone()),
                root_uri: Some(root.to_url()?),
                initialization_options: initialization_options.clone(),
                capabilities: ClientCapabilities {
                    text_document: Some(TextDocumentClientCapabilities {
                        color_provider: Some(GenericCapability {
                            dynamic_registration: Some(false),
                        }),
                        completion: Some(CompletionCapability {
                            completion_item: Some(CompletionItemCapability {
                                snippet_support: Some(has_snippet_support),
                                documentation_format: preferred_markup_kind.clone(),
                                // note that if this value was to be changed to true, then
                                // additional changes around edits should be made, as it currently
                                // just panics if it encounters a completion item of type
                                // InsertAndReplace.
                                insert_replace_support: Some(false),
                                ..CompletionItemCapability::default()
                            }),
                            ..CompletionCapability::default()
                        }),
                        code_action: Some(CodeActionCapability {
                            code_action_literal_support: Some(CodeActionLiteralSupport {
                                code_action_kind: CodeActionKindLiteralSupport {
                                    value_set: [
                                        CodeActionKind::QUICKFIX,
                                        CodeActionKind::REFACTOR,
                                        CodeActionKind::REFACTOR_EXTRACT,
                                        CodeActionKind::REFACTOR_INLINE,
                                        CodeActionKind::REFACTOR_REWRITE,
                                        CodeActionKind::SOURCE,
                                        CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                                    ]
                                    .iter()
                                    .map(|kind| kind.as_str().to_owned())
                                    .collect(),
                                },
                            }),
                            ..CodeActionCapability::default()
                        }),
                        signature_help: Some(SignatureHelpCapability {
                            signature_information: Some(SignatureInformationSettings {
                                active_parameter_support: None,
                                documentation_format: preferred_markup_kind.clone(),
                                parameter_information: Some(ParameterInformationSettings {
                                    label_offset_support: Some(true),
                                }),
                            }),
                            ..SignatureHelpCapability::default()
                        }),
                        declaration: Some(GotoCapability {
                            link_support: Some(true),
                            ..GotoCapability::default()
                        }),
                        definition: Some(GotoCapability {
                            link_support: Some(true),
                            ..GotoCapability::default()
                        }),
                        type_definition: Some(GotoCapability {
                            link_support: Some(true),
                            ..GotoCapability::default()
                        }),
                        implementation: Some(GotoCapability {
                            link_support: Some(true),
                            ..GotoCapability::default()
                        }),
                        publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                            related_information: Some(true),
                            ..PublishDiagnosticsClientCapabilities::default()
                        }),
                        code_lens: Some(GenericCapability {
                            dynamic_registration: Some(true),
                        }),
                        semantic_highlighting_capabilities: Some(
                            SemanticHighlightingClientCapability {
                                semantic_highlighting: true,
                            },
                        ),
                        hover: Some(HoverCapability {
                            content_format: preferred_markup_kind,
                            ..HoverCapability::default()
                        }),
                        ..TextDocumentClientCapabilities::default()
                    }),
                    workspace: Some(WorkspaceClientCapabilities {
                        apply_edit: Some(true),
                        did_change_watched_files: Some(GenericCapability {
                            dynamic_registration: Some(true),
                        }),
                        ..WorkspaceClientCapabilities::default()
                    }),
                    ..ClientCapabilities::default()
                },
                trace: Some(trace),
                workspace_folders: None,
            },
        )?;

        let initialize_result = InitializeResult::deserialize(&result)?;
        self.update_state(|state| {
            let server_name = initialize_result
                .server_info
                .as_ref()
                .map(|info| info.name.clone());
            match (server_name, initialization_options) {
                (Some(name), Some(options)) => {
                    state.initialization_options = state
                        .initialization_options
                        .combine(&json!({ name: options }));
                }
                _ => {}
            }

            state
                .capabilities
                .insert(language_id.clone(), initialize_result);

            Ok(())
        })?;

        if let Err(e) = self.register_cm_source(&language_id, &result) {
            let message = format!("LanguageClient: failed to register as NCM source: {}", e);
            error!("{}\n{:?}", message, e);
            self.vim()?.echoerr(&message)?;
        }
        if let Err(e) = self.register_ncm2_source(&language_id, &result) {
            let message = format!("LanguageClient: failed to register as NCM source: {}", e);
            error!("{}\n{:?}", message, e);
            self.vim()?.echoerr(&message)?;
        }
        if let Err(e) = self.parse_semantic_scopes(&language_id, &result) {
            let message = format!("LanguageClient: failed to parse semantic scopes: {}", e);
            error!("{}\n{:?}", message, e);
            self.vim()?.echoerr(&message)?;
        }

        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn initialized(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        self.update_semantic_highlight_tables(&language_id)?;
        self.get_client(&Some(language_id))?.notify(
            lsp_types::notification::Initialized::METHOD,
            InitializedParams {},
        )?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_hover(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let position = self.vim()?.get_position(params)?;

        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::HoverRequest::METHOD,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position,
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let hover = Option::<Hover>::deserialize(&result)?;
        if let Some(hover) = hover {
            if hover.to_display().is_empty() {
                self.vim()?
                    .echowarn("No hover information found for symbol")?;
                return Ok(Value::Null);
            }

            let hover_preview = self.get_config(|c| c.hover_preview)?;
            let use_preview = match hover_preview {
                HoverPreviewOption::Always => true,
                HoverPreviewOption::Never => false,
                HoverPreviewOption::Auto => hover.lines_len() > 1,
            };
            if use_preview {
                self.preview(&hover, "__LCNHover__")?
            } else {
                self.vim()?.echo_ellipsis(hover.to_string())?
            }
        }

        Ok(result)
    }

    /// Generic find locations, e.g, definitions, references.
    #[tracing::instrument(level = "info", skip(self))]
    pub fn find_locations(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let method: String =
            try_get("method", params)?.ok_or_else(|| anyhow!("method not found in request!"))?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let position = self.vim()?.get_position(params)?;
        let current_word = self.vim()?.get_current_word(params)?;
        let goto_cmd = self.vim()?.get_goto_cmd(params)?;

        let params = serde_json::to_value(TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            position,
        })?
        .combine(params);

        let result = self
            .get_client(&Some(language_id))?
            .call(&method, &params)?;

        if !self.vim()?.get_handle(&params)? {
            return Ok(result);
        }

        let response = Option::<GotoDefinitionResponse>::deserialize(&result)?;

        let locations = match response {
            None => vec![],
            Some(GotoDefinitionResponse::Scalar(loc)) => vec![loc],
            Some(GotoDefinitionResponse::Array(arr)) => arr,
            Some(GotoDefinitionResponse::Link(links)) => links
                .into_iter()
                .map(|link| Location::new(link.target_uri, link.target_selection_range))
                .collect(),
        };

        match locations.len() {
            0 => self.vim()?.echowarn("Not found!")?,
            1 => {
                let loc = locations.get(0).ok_or_else(|| anyhow!("Not found!"))?;
                let path = loc.uri.filepath()?.to_string_lossy().into_owned();
                self.edit(&goto_cmd, path)?;
                self.vim()?
                    .cursor(loc.range.start.line + 1, loc.range.start.character + 1)?;
                let cur_file: String = self.vim()?.eval("expand('%')")?;
                self.vim()?.echomsg_ellipsis(format!(
                    "{} {}:{}",
                    cur_file,
                    loc.range.start.line + 1,
                    loc.range.start.character + 1
                ))?;
            }
            _ => {
                let title = format!("[LC]: search for {}", current_word);
                self.present_list(&title, &locations)?
            }
        }

        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_rename(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let position = self.vim()?.get_position(params)?;
        let current_word = self.vim()?.get_current_word(params)?;
        let new_name: Option<String> = try_get("newName", params)?;

        let mut new_name = new_name.unwrap_or_default();
        if new_name.is_empty() {
            new_name = self
                .vim()?
                .rpcclient
                .call("s:getInput", ["Rename to: ", &current_word])?;
        }
        if new_name.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::Rename::METHOD,
            RenameParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: filename.to_url()?,
                    },
                    position,
                },
                new_name,
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }
        if result == Value::Null {
            return Ok(result);
        }

        let edit = WorkspaceEdit::deserialize(&result)?;
        self.apply_workspace_edit(&edit)?;

        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_document_symbol(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::DocumentSymbolRequest::METHOD,
            DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let syms = <Option<DocumentSymbolResponse>>::deserialize(&result)?;
        let title = format!("[LC]: symbols for {}", filename);

        match syms {
            Some(DocumentSymbolResponse::Flat(flat)) => {
                self.present_list(&title, &flat)?;
            }
            Some(DocumentSymbolResponse::Nested(nested)) => {
                let mut symbols = Vec::new();

                fn walk_document_symbol(
                    buffer: &mut Vec<lsp_types::DocumentSymbol>,
                    parent: Option<&lsp_types::DocumentSymbol>,
                    ds: &lsp_types::DocumentSymbol,
                ) {
                    let name = if let Some(parent) = parent {
                        format!("{}::{}", parent.name, ds.name)
                    } else {
                        ds.name.clone()
                    };

                    buffer.push(lsp_types::DocumentSymbol { name, ..ds.clone() });

                    if let Some(children) = &ds.children {
                        for child in children {
                            walk_document_symbol(buffer, Some(&ds), child);
                        }
                    }
                }

                for ds in &nested {
                    walk_document_symbol(&mut symbols, None, ds);
                }

                self.present_list(&title, &symbols)?;
            }
            _ => (),
        };

        Ok(result)
    }

    pub fn get_code_actions(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let range = Range::deserialize(&params["range"])?;

        // Unify filename.
        let filename = filename.canonicalize();

        let diagnostics: Vec<_> = self.get_state(|state| {
            state
                .diagnostics
                .get(&filename)
                .unwrap_or(&vec![])
                .iter()
                .filter(|dn| range.start >= dn.range.start && range.start < dn.range.end)
                .cloned()
                .collect()
        })?;

        let result: Value = self.get_client(&Some(language_id))?.call(
            lsp_types::request::CodeActionRequest::METHOD,
            CodeActionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                range,
                context: CodeActionContext {
                    diagnostics,
                    only: None,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )?;

        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn execute_code_action(&self, params: &Value) -> Result<Value> {
        let result = self.get_code_actions(params)?;
        let response = <Option<CodeActionResponse>>::deserialize(&result)?;
        let response: CodeActionResponse = response.unwrap_or_default();
        let kind: String =
            try_get("kind", params)?.ok_or_else(|| anyhow!("Missing kind parameter"))?;
        let action_kind = CodeActionKind::from(kind.clone());
        let actions: Vec<CodeActionOrCommand> = response.into_iter().filter(|a| matches!(a,
            CodeActionOrCommand::CodeAction(action) if action.kind.is_some() && action.kind.as_ref().unwrap() == &action_kind)
        ).collect();
        if actions.len() > 1 {
            return Err(anyhow!("Too many code actions found with kind {}", kind));
        }
        if actions.len() == 0 {
            return Err(anyhow!("No code actions found with kind {}", kind));
        }

        match actions.first().cloned() {
            Some(CodeActionOrCommand::CodeAction(action)) => {
                self.handle_code_action_selection(&[action], 0)?
            }
            _ => return Err(anyhow!("No code actions found with kind {}", kind)),
        }

        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_code_action(&self, params: &Value) -> Result<Value> {
        let result = self.get_code_actions(params)?;
        let response = <Option<CodeActionResponse>>::deserialize(&result)?;
        let response = response.unwrap_or_default();

        // Convert any Commands into CodeActions, so that the remainder of the handling can be
        // shared.
        let actions: Vec<_> = response
            .into_iter()
            .map(|action_or_command| match action_or_command {
                CodeActionOrCommand::Command(command) => CodeAction {
                    title: command.title.clone(),
                    kind: Some(command.command.clone().into()),
                    diagnostics: None,
                    edit: None,
                    command: Some(command),
                    ..CodeAction::default()
                },
                CodeActionOrCommand::CodeAction(action) => action,
            })
            .collect();

        self.update_state(|state| {
            state.stashed_code_action_actions = actions.clone();
            Ok(())
        })?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        self.present_actions("Code Actions", &actions, |idx| -> Result<()> {
            self.handle_code_action_selection(&actions, idx)
        })?;

        Ok(result)
    }

    fn handle_code_action_selection(&self, actions: &[CodeAction], idx: usize) -> Result<()> {
        match actions.get(idx) {
            Some(action) => {
                // Apply edit before command.
                if let Some(edit) = &action.edit {
                    self.apply_workspace_edit(edit)?;
                }

                if let Some(command) = &action.command {
                    if !self.try_handle_command_by_client(&command)? {
                        let params = json!({
                        "command": command.command,
                        "arguments": command.arguments,
                        });
                        self.workspace_execute_command(&params)?;
                    }
                }

                self.update_state(|state| {
                    state.stashed_code_action_actions = vec![];
                    Ok(())
                })?;
            }
            None => return Err(anyhow!("Code action not stashed, please try again")),
        };

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_completion(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let position = self.vim()?.get_position(params)?;

        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::Completion::METHOD,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position,
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_signature_help(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let position = self.vim()?.get_position(params)?;

        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::SignatureHelpRequest::METHOD,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position,
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }
        if result == Value::Null {
            return Ok(result);
        }

        let help = SignatureHelp::deserialize(result)?;
        if help.signatures.is_empty() {
            return Ok(Value::Null);
        }

        // active_signature may be negative value.
        // So if it is negative value, we convert it into zero.
        let active_signature_index = help.active_signature.unwrap_or(0).max(0) as usize;

        let active_signature = help
            .signatures
            .get(active_signature_index)
            .ok_or_else(|| anyhow!("Failed to get active signature"))?;

        // active_signature may be negative value.
        // So if it is negative value, we convert it into zero.
        let active_parameter_index = help.active_parameter.unwrap_or(0).max(0) as usize;

        let active_parameter: Option<&ParameterInformation>;
        if let Some(ref parameters) = active_signature.parameters {
            active_parameter = parameters.get(active_parameter_index);
        } else {
            active_parameter = None;
        }

        if let Some((begin, label, end)) = active_parameter.and_then(|active_parameter| {
            decode_parameter_label(&active_parameter.label, &active_signature.label).ok()
        }) {
            let cmd = format!(
                "echo | echon '{}' | echohl WarningMsg | echon '{}' | echohl None | echon '{}'",
                begin, label, end
            );
            self.vim()?.command(&cmd)?;
        } else {
            self.vim()?.echo(&active_signature.label)?;
        }

        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_definition(&self, params: &Value) -> Result<Value> {
        let params = json!({
            "method": lsp_types::request::GotoDefinition::METHOD,
        })
        .combine(params);
        let result = self.find_locations(&params)?;
        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_references(&self, params: &Value) -> Result<Value> {
        let include_declaration: bool = try_get("includeDeclaration", params)?.unwrap_or(true);
        let params = json!({
            "method": lsp_types::request::References::METHOD,
            "context": ReferenceContext {
                include_declaration,
            }
        })
        .combine(params);
        let result = self.find_locations(&params)?;
        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_formatting(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let tab_size = self.vim()?.get_tab_size()?;
        let insert_spaces = self.vim()?.get_insert_spaces(&filename)?;
        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::Formatting::METHOD,
            DocumentFormattingParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                options: FormattingOptions {
                    tab_size,
                    insert_spaces,
                    properties: HashMap::new(),
                    ..FormattingOptions::default()
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let text_edits = <Option<Vec<TextEdit>>>::deserialize(&result)?;
        let text_edits = text_edits.unwrap_or_default();
        let edit = lsp_types::WorkspaceEdit {
            changes: Some(hashmap! {filename.to_url()? => text_edits}),
            document_changes: None,
        };
        self.apply_workspace_edit(&edit)?;
        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_range_formatting(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let start_line = try_get("range_start_line", params)?
            .map_or_else(|| self.vim()?.eval("LSP#range_start_line()"), Ok)?;
        let end_line = try_get("range_end_line", params)?
            .map_or_else(|| self.vim()?.eval("LSP#range_end_line()"), Ok)?;

        let tab_size = self.vim()?.get_tab_size()?;
        let insert_spaces = self.vim()?.get_insert_spaces(&filename)?;
        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::RangeFormatting::METHOD,
            DocumentRangeFormattingParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                options: FormattingOptions {
                    tab_size,
                    insert_spaces,
                    properties: HashMap::new(),
                    ..FormattingOptions::default()
                },
                range: Range {
                    start: Position {
                        line: start_line,
                        character: 0,
                    },
                    end: Position {
                        line: end_line,
                        character: 0,
                    },
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let text_edits = <Option<Vec<TextEdit>>>::deserialize(&result)?;
        let text_edits = text_edits.unwrap_or_default();
        let edit = lsp_types::WorkspaceEdit {
            changes: Some(hashmap! {filename.to_url()? => text_edits}),
            document_changes: None,
        };
        self.apply_workspace_edit(&edit)?;
        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn completion_item_resolve(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let has_capability =
            self.get_state(|state| match state.capabilities.get(&language_id) {
                None => false,
                Some(result) => result
                    .capabilities
                    .completion_provider
                    .as_ref()
                    .map(|cp| cp.resolve_provider.unwrap_or_default())
                    .unwrap_or_default(),
            })?;
        if !has_capability {
            return Ok(Value::Null);
        }

        let completion_item: CompletionItem = try_get("completionItem", params)?
            .ok_or_else(|| anyhow!("completionItem not found in request!"))?;
        let pumpos: Value =
            try_get("pumpos", params)?.ok_or_else(|| anyhow!("pumpos not found in request!"))?;

        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::ResolveCompletionItem::METHOD,
            completion_item,
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let item = CompletionItem::deserialize(result)?;
        match item.documentation {
            None => return Ok(Value::Null),
            Some(Documentation::String(s)) if s.is_empty() => return Ok(Value::Null),
            Some(Documentation::MarkupContent(m)) if m.value.is_empty() => return Ok(Value::Null),
            _ => self.vim()?.rpcclient.notify(
                "s:ShowCompletionItemDocumentation",
                json!([item.documentation, pumpos]),
            )?,
        }

        Ok(Value::Null)
    }

    // shows a list of actions for the user to choose one.
    fn present_actions<T, F>(&self, title: &str, actions: &[T], callback: F) -> Result<()>
    where
        T: ListItem,
        F: Fn(usize) -> Result<()>,
    {
        if actions.is_empty() {
            return Err(anyhow!("No code actions found at point"));
        }

        let cwd: String = self.vim()?.eval("getcwd()")?;
        let actions: Result<Vec<String>> = actions
            .iter()
            .map(|it| ListItem::string_item(it, self, &cwd))
            .collect();

        match self.get_config(|c| c.selection_ui)? {
            SelectionUI::Funcref => {
                self.vim()?.rpcclient.notify(
                    "s:selectionUI_funcref",
                    json!([actions?, NOTIFICATION_FZF_SINK_COMMAND]),
                )?;
            }
            SelectionUI::Quickfix | SelectionUI::LocationList => {
                let mut actions: Vec<String> = actions?
                    .iter_mut()
                    .enumerate()
                    .map(|(idx, it)| format!("{}) {}", idx + 1, it))
                    .collect();
                let mut options = vec![title.to_string()];
                options.append(&mut actions);

                let index: Option<usize> = self.vim()?.rpcclient.call("s:inputlist", options)?;
                if let Some(index) = index {
                    return callback(index - 1);
                }
            }
        }

        Ok(())
    }

    // shows a list of items, used for things like diagnostics or things that do not need a user
    // selection.
    pub fn present_list<T>(&self, title: &str, items: &[T]) -> Result<()>
    where
        T: ListItem,
    {
        let selection_ui = self.get_config(|c| c.selection_ui)?;
        let selection_ui_auto_open = self.get_config(|c| c.selection_ui_auto_open)?;

        match selection_ui {
            SelectionUI::Funcref => {
                let cwd: String = self.vim()?.eval("getcwd()")?;
                let source: Result<Vec<_>> = items
                    .iter()
                    .map(|it| ListItem::string_item(it, self, &cwd))
                    .collect();
                let source = source?;

                self.vim()?.rpcclient.notify(
                    "s:selectionUI_funcref",
                    json!([source, format!("s:{}", NOTIFICATION_FZF_SINK_LOCATION)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Result<Vec<_>> = items
                    .iter()
                    .map(|it| ListItem::quickfix_item(it, self))
                    .collect();
                let list = list?;
                self.vim()?.setqflist(&list, " ", title)?;
                if selection_ui_auto_open {
                    self.vim()?.command("botright copen")?;
                }
                self.vim()?.echo("Populated quickfix list.")?;
            }
            SelectionUI::LocationList => {
                let list: Result<Vec<_>> = items
                    .iter()
                    .map(|it| ListItem::quickfix_item(it, self))
                    .collect();
                let list = list?;
                self.vim()?.setloclist(&list, " ", title)?;
                if selection_ui_auto_open {
                    self.vim()?.command("lopen")?;
                }
                self.vim()?.echo("Populated location list.")?;
            }
        }

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn workspace_symbol(&self, params: &Value) -> Result<Value> {
        self.text_document_did_change(params)?;
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let query = try_get("query", params)?.unwrap_or_default();
        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::WorkspaceSymbol::METHOD,
            WorkspaceSymbolParams {
                query,
                partial_result_params: PartialResultParams::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let symbols = <Vec<SymbolInformation>>::deserialize(&result)?;
        let title = "[LC]: workspace symbols";

        self.present_list(title, &symbols)?;
        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn workspace_execute_command(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let command: String =
            try_get("command", params)?.ok_or_else(|| anyhow!("command not found in request!"))?;
        let arguments: Vec<Value> = try_get("arguments", params)?.unwrap_or_default();

        let result = self.get_client(&Some(language_id))?.call(
            lsp_types::request::ExecuteCommand::METHOD,
            ExecuteCommandParams {
                command,
                arguments,
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        )?;
        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn workspace_apply_edit(&self, params: &Value) -> Result<Value> {
        let params = ApplyWorkspaceEditParams::deserialize(params)?;
        self.apply_workspace_edit(&params.edit)?;
        Ok(serde_json::to_value(ApplyWorkspaceEditResponse {
            applied: true,
        })?)
    }

    pub fn workspace_did_change_configuration(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let settings: Value = try_get("settings", params)?.unwrap_or_default();

        self.get_client(&Some(language_id))?.notify(
            lsp_types::notification::DidChangeConfiguration::METHOD,
            DidChangeConfigurationParams { settings },
        )?;
        Ok(())
    }

    pub fn handle_code_lens_action(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let line = self.vim()?.get_position(params)?.line;

        let code_lens: Vec<CodeLens> = self.get_state(|state| {
            state
                .code_lens
                .get(&filename)
                .cloned()
                .unwrap_or_else(Vec::new)
                .into_iter()
                .filter(|action| action.range.start.line == line)
                .collect()
        })?;
        if code_lens.is_empty() {
            warn!("No actions associated with this codeLens");
            return Ok(Value::Null);
        }

        let actions: Result<Vec<CodeAction>> = code_lens
            .iter()
            .map(|cl| match &cl.command {
                None => Err(anyhow!("no command, skipping")),
                Some(cmd) => Ok(CodeAction {
                    kind: Some(cmd.command.clone().into()),
                    title: cmd.title.clone(),
                    command: cl.clone().command,
                    diagnostics: None,
                    edit: None,
                    is_preferred: None,
                    disabled: None,
                    data: None,
                }),
            })
            .filter(Result::is_ok)
            .collect();
        let actions = actions?;

        self.update_state(|state| {
            state.stashed_code_action_actions = actions.clone();
            Ok(())
        })?;

        let source: Result<Vec<Command>> = actions
            .iter()
            .map(|it| match &it.command {
                None => Err(anyhow!("expected a command, found none")),
                Some(cmd) => Ok(cmd.clone()),
            })
            .collect();
        // every item in `actions` should have a command, as we filtered the ones that didn't have
        // one before. If we happen to encounter one that does not have a command, we just error,
        // as this is unexpected behaviour and could potentially lead to triggering the incorrect
        // code action, as the index may be incorrect.
        let source = source?;

        self.present_actions("Code Lens Actions", &source, |idx| -> Result<()> {
            self.handle_code_action_selection(&actions, idx)
        })?;

        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn progress(&self, params: &Value) -> Result<()> {
        let params = ProgressParams::deserialize(params)?;
        let message = match params.value {
            ProgressParamsValue::WorkDone(wd) => match wd {
                WorkDoneProgress::Begin(r) => {
                    Some(format!("{} {}", r.title, r.message.unwrap_or_default()))
                }
                WorkDoneProgress::Report(r) => r.message,
                // WorkDoneProgress::End has no value, so we return Done, otherwise the previous
                // message would be left in screen and it would appear as if it didn't ever finish.
                WorkDoneProgress::End(_) => Some("Done".into()),
            },
        };

        if message.is_none() {
            return Ok(());
        }

        let token = match params.token {
            // number is a not a particularly useful token to report to the user, so we just use
            // INFO instead.
            NumberOrString::Number(_) => "INFO".to_string(),
            NumberOrString::String(s) => s,
        };

        let message = format!("{}: {}", token, message.unwrap_or_default());
        self.vim()?.echomsg(&message)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_code_lens(&self, params: &Value) -> Result<Value> {
        let use_virtual_text = self.get_config(|c| c.use_virtual_text.clone())?;
        if UseVirtualText::No == use_virtual_text || UseVirtualText::Diagnostics == use_virtual_text
        {
            return Ok(Value::Null);
        }

        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let capabilities = self.get_state(|state| state.capabilities.clone())?;
        if let Some(initialize_result) = capabilities.get(&language_id) {
            // XXX: the capabilities state field stores the initialize result, not the capabilities
            // themselves, so we need to deserialize to InitializeResult.
            let capabilities = initialize_result.capabilities.clone();

            if let Some(code_lens_provider) = capabilities.code_lens_provider {
                let client = self.get_client(&Some(language_id))?;
                let input = lsp_types::CodeLensParams {
                    text_document: TextDocumentIdentifier {
                        uri: filename.to_url()?,
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                };

                let results: Value =
                    client.call(lsp_types::request::CodeLensRequest::METHOD, &input)?;
                let code_lens = <Option<Vec<CodeLens>>>::deserialize(results)?;
                let mut code_lens: Vec<CodeLens> = code_lens.unwrap_or_default();

                if code_lens_provider.resolve_provider.unwrap_or_default() {
                    code_lens = code_lens
                        .into_iter()
                        .map(|cl| {
                            if cl.data.is_none() {
                                return cl;
                            }

                            client
                                .call(lsp_types::request::CodeLensResolve::METHOD, &cl)
                                .unwrap_or(cl)
                        })
                        .collect();
                }

                self.update_state(|state| {
                    state.code_lens.insert(filename.to_owned(), code_lens);
                    Ok(Value::Null)
                })?;
            }
        }

        self.draw_virtual_texts(&params)?;

        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_did_open(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let text = self.vim()?.get_text(&filename)?;
        let set_omnifunc: bool = self
            .vim()?
            .eval("s:GetVar('LanguageClient_setOmnifunc', v:true)")?;

        let text_document = TextDocumentItem {
            uri: filename.to_url()?,
            language_id: language_id.clone(),
            version: 0,
            text: text.join("\n"),
        };

        self.update_state(|state| {
            Ok(state
                .text_documents
                .insert(filename.clone(), text_document.clone()))
        })?;

        self.get_client(&Some(language_id.clone()))?.notify(
            lsp_types::notification::DidOpenTextDocument::METHOD,
            DidOpenTextDocumentParams { text_document },
        )?;

        if set_omnifunc {
            self.vim()?
                .command("setlocal omnifunc=LanguageClient#complete")?;
        }
        let root =
            self.get_state(|state| state.roots.get(&language_id).cloned().unwrap_or_default())?;
        self.vim()?.rpcclient.notify(
            "setbufvar",
            json!([filename, "LanguageClient_projectRoot", root]),
        )?;
        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageClientTextDocumentDidOpenPost")?;

        self.text_document_code_lens(params)?;
        self.text_document_inlay_hints(&language_id, &filename)?;

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_did_change(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        if !self.get_state(|state| state.text_documents.contains_key(&filename))? {
            info!("Not opened yet. Switching to didOpen.");
            return self.text_document_did_open(params);
        }

        let text = self.vim()?.get_text(&filename)?.join("\n");
        let text_state = self.get_state(|state| {
            state
                .text_documents
                .get(&filename)
                .map(|d| d.text.clone())
                .unwrap_or_default()
        })?;
        if text == text_state {
            return Ok(());
        }

        let change_throttle = self.get_config(|c| c.change_throttle.is_some())?;
        let version = self.update_state(|state| {
            let document = state
                .text_documents
                .get_mut(&filename)
                .ok_or_else(|| anyhow!("Failed to get TextDocumentItem! filename: {}", filename))?;

            let version = document.version + 1;
            document.version = version;
            document.text = text.clone();

            if change_throttle {
                let metadata = state
                    .text_documents_metadata
                    .entry(filename.clone())
                    .or_insert_with(TextDocumentItemMetadata::default);
                metadata.last_change = Instant::now();
            }
            Ok(version)
        })?;

        self.get_client(&Some(language_id.clone()))?.notify(
            lsp_types::notification::DidChangeTextDocument::METHOD,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: filename.to_url()?,
                    version: Some(version),
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text,
                }],
            },
        )?;

        self.text_document_code_lens(params)?;
        self.text_document_inlay_hints(&language_id, &filename)?;

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_did_save(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        if !self.get_config(|c| c.server_commands.contains_key(&language_id))? {
            return Ok(());
        }

        let uri = filename.to_url()?;

        self.get_client(&Some(language_id.clone()))?.notify(
            lsp_types::notification::DidSaveTextDocument::METHOD,
            DidSaveTextDocumentParams {
                text: None,
                text_document: TextDocumentIdentifier { uri },
            },
        )?;

        self.draw_virtual_texts(params)?;

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_did_close(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        self.get_client(&Some(language_id))?.notify(
            lsp_types::notification::DidCloseTextDocument::METHOD,
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
            },
        )?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_publish_diagnostics(&self, params: &Value) -> Result<()> {
        let params = PublishDiagnosticsParams::deserialize(params)?;
        if !self.get_config(|c| c.diagnostics_enable)? {
            return Ok(());
        }

        let mut filename = params.uri.filepath()?.to_string_lossy().into_owned();
        // Workaround bug: remove first '/' in case of '/C:/blabla'.
        if filename.starts_with('/') && filename.chars().nth(2) == Some(':') {
            filename.remove(0);
        }
        // Unify name to avoid mismatch due to case insensitivity.
        let filename = filename.canonicalize();

        let diagnostics_max_severity = self.get_config(|c| c.diagnostics_max_severity)?;
        let ignore_sources = self.get_config(|c| c.diagnostics_ignore_sources.clone())?;
        let mut diagnostics = params
            .diagnostics
            .iter()
            .filter(|&diagnostic| {
                if let Some(source) = &diagnostic.source {
                    if ignore_sources.contains(source) {
                        return false;
                    }
                }
                diagnostic.severity.unwrap_or(DiagnosticSeverity::Hint) <= diagnostics_max_severity
            })
            .map(Clone::clone)
            .collect::<Vec<_>>();

        self.update_state(|state| {
            state
                .diagnostics
                .insert(filename.clone(), diagnostics.clone());
            Ok(())
        })?;
        self.update_quickfixlist()?;

        let mut severity_count: HashMap<String, u64> = [
            (
                DiagnosticSeverity::Error
                    .to_quickfix_entry_type()
                    .to_string(),
                0,
            ),
            (
                DiagnosticSeverity::Warning
                    .to_quickfix_entry_type()
                    .to_string(),
                0,
            ),
            (
                DiagnosticSeverity::Information
                    .to_quickfix_entry_type()
                    .to_string(),
                0,
            ),
            (
                DiagnosticSeverity::Hint
                    .to_quickfix_entry_type()
                    .to_string(),
                0,
            ),
        ]
        .iter()
        .cloned()
        .collect();

        for diagnostic in diagnostics.iter() {
            let severity = diagnostic
                .severity
                .unwrap_or(DiagnosticSeverity::Hint)
                .to_quickfix_entry_type()
                .to_string();
            let count = severity_count.entry(severity).or_insert(0);
            *count += 1;
        }

        if let Ok(bufnr) = self
            .vim()?
            .eval::<_, Bufnr>(format!("bufnr('{}')", filename))
        {
            // Some Language Server diagnoses non-opened buffer, so we must check if buffer exists.
            if bufnr > 0 {
                self.vim()?.rpcclient.notify(
                    "setbufvar",
                    json!([filename, VIM_STATUS_LINE_DIAGNOSTICS_COUNTS, severity_count]),
                )?;
            }
        }

        let current_filename: String = self.vim()?.get_filename(&Value::Null)?;
        if filename != current_filename.canonicalize() {
            return Ok(());
        }

        // Sort diagnostics as pre-process for display.
        // First sort by line.
        // Then severity descending. Error should come last since when processing item comes
        // later will override its precedence.
        // Then by character descending.
        diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.range.start.line,
                -(diagnostic.severity.unwrap_or(DiagnosticSeverity::Hint) as i8),
                -(diagnostic.range.start.line as i64),
            )
        });

        self.process_diagnostics(&current_filename, &diagnostics)?;
        self.handle_cursor_moved(&Value::Null, true)?;
        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageClientDiagnosticsChanged")?;

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn text_document_semantic_highlight(&self, params: &Value) -> Result<()> {
        let mut params = SemanticHighlightingParams::deserialize(params)?;

        // TODO: Do we need to handle the versioning of the file?
        let mut filename = params
            .text_document
            .uri
            .filepath()?
            .to_string_lossy()
            .into_owned();
        // Workaround bug: remove first '/' in case of '/C:/blabla'.
        if filename.starts_with('/') && filename.chars().nth(2) == Some(':') {
            filename.remove(0);
        }
        // Unify name to avoid mismatch due to case insensitivity.
        let filename = filename.canonicalize();
        let language_id = self.vim()?.get_language_id(&filename, &Value::Null)?;

        let opt_hl_table = self.get_state(|state| {
            state
                .semantic_scope_to_hl_group_table
                .get(&language_id)
                .cloned()
        })?;

        // Sort lines in ascending order
        params.lines.sort_by(|a, b| a.line.cmp(&b.line));

        // Remove obviously invalid values
        while let Some(line_info) = params.lines.first() {
            if line_info.line >= 0 {
                break;
            } else {
                warn!(
                    "Invalid Semantic Highlight Line: {}",
                    params.lines.remove(0).line
                );
            }
        }

        let semantic_hl_state = TextDocumentSemanticHighlightState {
            last_version: params.text_document.version,
            symbols: params.lines,
            highlights: None,
        };

        if let Some(hl_table) = opt_hl_table {
            let ns_id = self.get_or_create_namespace(&LCNamespace::SemanticHighlight)?;

            let buffer = self.vim()?.get_bufnr(&filename, &Value::Null)?;

            if buffer == -1 {
                error!(
                    "Received Semantic Highlighting for non-open buffer: {}",
                    filename
                );
                return Ok(());
            }

            /*
             * Currently servers update entire regions of text at a time or a
             * single line so simply clear between the first and last line to
             * ensure no highlights are left dangling
             */
            let mut clear_region: Option<(u64, u64)> = None;
            let mut highlights = Vec::with_capacity(semantic_hl_state.symbols.len());

            for line in &semantic_hl_state.symbols {
                if let Some(tokens) = &line.tokens {
                    for token in tokens {
                        if token.length == 0 {
                            continue;
                        }

                        if let Some(Some(group)) = hl_table.get(token.scope as usize) {
                            highlights.push(Highlight {
                                line: line.line as u64,
                                character_start: token.character as u64,
                                character_end: token.character as u64 + token.length as u64,
                                group: group.clone(),
                                text: String::new(),
                            });
                        }
                    }

                    match clear_region {
                        Some((begin, _)) => {
                            clear_region = Some((begin, line.line as u64 + 1));
                        }
                        None => {
                            clear_region = Some((line.line as u64, line.line as u64 + 1));
                        }
                    }
                }
            }

            info!(
                "Semantic Highlighting Region [{}, {}]:",
                semantic_hl_state
                    .symbols
                    .first()
                    .map_or(-1, |h| h.line as i64),
                semantic_hl_state
                    .symbols
                    .last()
                    .map_or(-1, |h| h.line as i64)
            );

            info!(
                "Semantic Highlighting Region (Parsed) [{}, {}]:",
                highlights.first().map_or(-1, |h| h.line as i64),
                highlights.last().map_or(-1, |h| h.line as i64)
            );

            let mut clears = Vec::new();
            if let Some((begin, end)) = clear_region {
                clears.push(ClearNamespace {
                    line_start: begin,
                    line_end: end,
                });
            }

            let mut num_semantic_hls = 0;
            let num_new_semantic_hls = highlights.len();

            self.update_state(|state| {
                state.vim.rpcclient.notify(
                    "s:ApplySemanticHighlights",
                    json!([buffer, ns_id, clears, highlights]),
                )?;

                let old_semantic_hl_state = state
                    .semantic_highlights
                    .insert(language_id.clone(), semantic_hl_state);

                let semantic_hl_state = state.semantic_highlights.get_mut(&language_id).unwrap();

                let mut combined_hls = Vec::with_capacity(highlights.len());

                let mut existing_hls = old_semantic_hl_state
                    .map_or(Vec::new(), |hl_state| {
                        hl_state.highlights.unwrap_or_default()
                    })
                    .into_iter()
                    .peekable();

                let mut new_hls = highlights.into_iter().peekable();

                // Incrementally update the highlighting
                loop {
                    match (existing_hls.peek(), new_hls.peek()) {
                        (Some(existing_hl), Some(new_hl)) => {
                            use std::cmp::Ordering;

                            match existing_hl.line.cmp(&new_hl.line) {
                                Ordering::Less => {
                                    if clear_region.unwrap_or((0, 0)).0 <= existing_hl.line
                                        && existing_hl.line < clear_region.unwrap_or((0, 0)).1
                                    {
                                        // within clear region, this highlight gets cleared
                                        existing_hls.next();
                                    } else {
                                        combined_hls
                                            .push(existing_hls.next().expect("unreachable"));
                                    }
                                }
                                Ordering::Greater => {
                                    combined_hls.push(new_hls.next().expect("unreachable"));
                                }
                                Ordering::Equal => {
                                    // existing highlight on same line as new, it gets cleared
                                    existing_hls.next();
                                }
                            }
                        }
                        (Some(_), None) => {
                            combined_hls.push(existing_hls.next().expect("unreachable"));
                        }
                        (None, Some(_)) => {
                            combined_hls.push(new_hls.next().expect("unreachable"));
                        }
                        (None, None) => {
                            break;
                        }
                    }
                }

                num_semantic_hls = combined_hls.len();

                semantic_hl_state.highlights = Some(combined_hls);

                Ok(())
            })?;

            info!(
                "Applied Semantic Highlighting for {} Symbols ({} new)",
                num_semantic_hls, num_new_semantic_hls
            )
        } else {
            self.update_state(|state| {
                state
                    .semantic_highlights
                    .insert(language_id.clone(), semantic_hl_state);
                Ok(())
            })?;
        }

        Ok(())
    }

    // logs a message to with the specified level to the log file if the threshold is below the
    // message's level.
    #[tracing::instrument(level = "info", skip(self))]
    pub fn window_log_message(&self, params: &Value) -> Result<()> {
        let params = LogMessageParams::deserialize(params)?;
        let threshold = self.get_config(|c| c.window_log_message_level)?;
        if params.typ.to_int()? > threshold.to_int()? {
            return Ok(());
        }

        match params.typ {
            MessageType::Error => error!("{}", params.message),
            MessageType::Warning => warn!("{}", params.message),
            MessageType::Info => info!("{}", params.message),
            MessageType::Log => debug!("{}", params.message),
        };

        Ok(())
    }

    // shows the given message in vim.
    #[tracing::instrument(level = "info", skip(self))]
    pub fn window_show_message(&self, params: &Value) -> Result<()> {
        let params = ShowMessageParams::deserialize(params)?;
        let msg = format!("[{:?}] {}", params.typ, params.message);

        match params.typ {
            MessageType::Error => self.vim()?.echoerr(msg)?,
            MessageType::Warning => self.vim()?.echowarn(msg)?,
            MessageType::Info => self.vim()?.echomsg(msg)?,
            MessageType::Log => self.vim()?.echomsg(msg)?,
        };

        Ok(())
    }

    // TODO: change this to use the show_acions method
    #[tracing::instrument(level = "info", skip(self))]
    pub fn window_show_message_request(&self, params: &Value) -> Result<Value> {
        let mut v = Value::Null;
        let msg_params = ShowMessageRequestParams::deserialize(params)?;
        let msg = format!("[{:?}] {}", msg_params.typ, msg_params.message);
        let msg_actions = msg_params.actions.unwrap_or_default();
        if msg_actions.is_empty() {
            self.vim()?.echomsg(&msg)?;
        } else {
            let mut options = Vec::with_capacity(msg_actions.len() + 1);
            options.push(msg);
            options.extend(
                msg_actions
                    .iter()
                    .enumerate()
                    .map(|(i, item)| format!("{}) {}", i + 1, item.title)),
            );

            let index: Option<usize> = self.vim()?.rpcclient.call("s:inputlist", options)?;
            if let Some(index) = index {
                v = serde_json::to_value(msg_actions.get(index - 1))?;
            }
        }

        Ok(v)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn client_register_capability(&self, language_id: &str, params: &Value) -> Result<Value> {
        let params = RegistrationParams::deserialize(params)?;
        for r in &params.registrations {
            match r.method.as_str() {
                lsp_types::notification::DidChangeWatchedFiles::METHOD => {
                    let opt = DidChangeWatchedFilesRegistrationOptions::deserialize(
                        r.register_options.as_ref().unwrap_or(&Value::Null),
                    )?;
                    if !self.get_state(|state| state.watchers.contains_key(language_id))? {
                        let (watcher_tx, watcher_rx) = mpsc::channel();
                        // TODO: configurable duration.
                        let watcher = FSWatch::new(watcher_tx, Duration::from_secs(2))?;
                        self.update_state(|state| {
                            state.watchers.insert(language_id.to_owned(), watcher);
                            state.watcher_rxs.insert(language_id.to_owned(), watcher_rx);
                            Ok(())
                        })?;
                    }

                    self.update_state(|state| {
                        if let Some(ref mut watcher) = state.watchers.get_mut(language_id) {
                            for w in &opt.watchers {
                                info!("Watching glob pattern: {}", &w.glob_pattern);
                                for entry in glob(&w.glob_pattern)? {
                                    match entry {
                                        Ok(path) => {
                                            if path.is_dir() {
                                                watcher.watch_dir(
                                                    &path,
                                                    notify::RecursiveMode::Recursive,
                                                )?;
                                            } else {
                                                watcher.watch_file(&path)?;
                                            };
                                            info!("Start watching path {:?}", path);
                                        }
                                        Err(e) => {
                                            warn!("Error globbing for {}: {}", w.glob_pattern, e)
                                        }
                                    }
                                }
                            }
                        }
                        Ok(())
                    })?;
                }
                _ => {
                    warn!("Unknown registration: {:?}", r);
                }
            }
        }

        self.update_state(|state| {
            state.registrations.extend(params.registrations);
            Ok(())
        })?;
        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn client_unregister_capability(&self, language_id: &str, params: &Value) -> Result<Value> {
        let params = UnregistrationParams::deserialize(params)?;
        let mut regs_removed = vec![];
        for r in &params.unregisterations {
            if let Some(idx) = self.get_state(|state| {
                state
                    .registrations
                    .iter()
                    .position(|i| i.id == r.id && i.method == r.method)
            })? {
                regs_removed
                    .push(self.update_state(|state| Ok(state.registrations.swap_remove(idx)))?);
            }
        }

        for r in &regs_removed {
            match r.method.as_str() {
                lsp_types::notification::DidChangeWatchedFiles::METHOD => {
                    let opt = DidChangeWatchedFilesRegistrationOptions::deserialize(
                        r.register_options.as_ref().unwrap_or(&Value::Null),
                    )?;
                    self.update_state(|state| {
                        if let Some(ref mut watcher) = state.watchers.get_mut(language_id) {
                            for w in opt.watchers {
                                watcher.unwatch(w.glob_pattern)?;
                            }
                        }
                        Ok(())
                    })?;
                }
                _ => {
                    warn!("Unknown registration: {:?}", r);
                }
            }
        }

        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn shutdown(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let _: () = self
            .get_client(&Some(language_id.clone()))?
            .call(lsp_types::request::Shutdown::METHOD, Value::Null)?;

        self.vim()?
            .rpcclient
            .notify("setbufvar", json!([filename, VIM_IS_SERVER_RUNNING, 0]))?;

        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn exit(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let result = self
            .get_client(&Some(language_id.clone()))?
            .notify(lsp_types::notification::Exit::METHOD, Value::Null);
        if let Err(err) = result {
            error!("Error: {:?}", err);
        }

        if let Err(err) = self.cleanup(&language_id) {
            error!("Error: {:?}", err);
        }

        Ok(())
    }

    /////// Extensions by this plugin ///////

    #[tracing::instrument(level = "info", skip(self))]
    pub fn get_client_state(&self, _params: &Value) -> Result<Value> {
        let s = self.get_state(|state| serde_json::to_string(state))??;
        Ok(Value::String(s))
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn is_alive(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let is_alive =
            self.get_state(|state| state.clients.contains_key(&Some(language_id.clone())))?;
        Ok(Value::Bool(is_alive))
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn register_server_commands(&self, params: &Value) -> Result<Value> {
        let commands = HashMap::<String, ServerCommand>::deserialize(params)?;
        self.update_config(|c| c.server_commands.extend(commands))?;
        let exp = format!(
            "let g:LanguageClient_serverCommands={}",
            serde_json::to_string(&self.get_config(|c| c.server_commands.clone())?)?
        );
        self.vim()?.command(&exp)?;
        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn set_logging_level(&self, params: &Value) -> Result<Value> {
        let logging_level =
            try_get("loggingLevel", params)?.ok_or_else(|| anyhow!("loggingLevel not found!"))?;
        self.update_state(|state| {
            state.logger.set_level(logging_level)?;
            Ok(())
        })?;
        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn set_diagnostics_list(&self, params: &Value) -> Result<Value> {
        let diagnostics_list = try_get("diagnosticsList", params)?
            .ok_or_else(|| anyhow!("diagnosticsList not found!"))?;
        self.update_config(|c| c.diagnostics_list = diagnostics_list)?;
        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn register_handlers(&self, params: &Value) -> Result<Value> {
        let handlers: Result<HashMap<String, String>> = params
            .as_object()
            .ok_or_else(|| anyhow!("Invalid arguments!"))?
            .iter()
            .filter_map(|(k, v)| {
                if *k == "bufnr" || *k == "languageId" {
                    return None;
                }

                if let serde_json::Value::String(v) = v {
                    Some(Ok((k.clone(), v.clone())))
                } else {
                    None
                }
            })
            .collect();
        let handlers = handlers?;
        self.update_state(|state| {
            state.user_handlers.extend(handlers);
            Ok(())
        })?;
        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn omnicomplete(&self, params: &Value) -> Result<Value> {
        let result = self.text_document_completion(params)?;
        let result = <Option<CompletionResponse>>::deserialize(result)?;
        let result = result.unwrap_or_else(|| CompletionResponse::Array(vec![]));
        let matches = match result {
            CompletionResponse::Array(arr) => arr,
            CompletionResponse::List(list) => list.items,
        };

        let complete_position: Option<u64> = try_get("complete_position", params)?;

        let matches: Result<Vec<VimCompleteItem>> = matches
            .iter()
            .map(|item| VimCompleteItem::from_lsp(item, complete_position))
            .collect();
        let matches = matches?;
        Ok(serde_json::to_value(matches)?)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn handle_buf_new_file(&self, params: &Value) -> Result<()> {
        if self.vim()?.get_filename(params)?.is_empty() {
            return Ok(());
        }

        let auto_start: u8 = self
            .vim()?
            .eval("!!get(g:, 'LanguageClient_autoStart', 1)")?;
        if auto_start == 1 {
            let ret = self.start_server(params);
            // This is triggered from autocmd, silent all errors.
            if let Err(err) = ret {
                warn!("Failed to start language server automatically. {}", err);
            }
            self.text_document_did_open(params)?;
        }

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn handle_buf_enter(&self, params: &Value) -> Result<()> {
        if self.vim()?.get_filename(params)?.is_empty() {
            return Ok(());
        }

        let filename = self.vim()?.get_filename(params)?.canonicalize();
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        if self.get_state(|state| state.clients.contains_key(&Some(language_id.clone())))? {
            self.vim()?
                .rpcclient
                .notify("setbufvar", json!([filename, VIM_IS_SERVER_RUNNING, 1]))?;
        } else {
            self.vim()?
                .rpcclient
                .notify("setbufvar", json!([filename, VIM_IS_SERVER_RUNNING, 0]))?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn handle_file_type(&self, params: &Value) -> Result<()> {
        if self.vim()?.get_filename(params)?.is_empty() {
            return Ok(());
        }

        let filename = self.vim()?.get_filename(params)?.canonicalize();
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        if self.get_state(|state| state.clients.contains_key(&Some(language_id.clone())))? {
            self.text_document_did_open(params)?;

            if let Some(diagnostics) =
                self.get_state(|state| state.diagnostics.get(&filename).cloned())?
            {
                self.process_diagnostics(&filename, &diagnostics)?;
                self.handle_cursor_moved(params, true)?;
            }
        } else {
            let auto_start: u8 = self
                .vim()?
                .eval("!!get(g:, 'LanguageClient_autoStart', 1)")?;
            if auto_start == 1 {
                let ret = self.start_server(params);
                // This is triggered from autocmd, silent all errors.
                if let Err(err) = ret {
                    warn!("Failed to start language server automatically. {}", err);
                }
                self.text_document_did_open(params)?;
            }
        }

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn handle_text_changed(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        if !self.get_config(|c| c.server_commands.contains_key(&language_id))? {
            return Ok(());
        }

        let change_throttle = self.get_config(|c| c.change_throttle)?;
        let skip_notification = self.get_state(|state| {
            if let Some(metadata) = state.text_documents_metadata.get(&filename) {
                if let Some(throttle) = change_throttle {
                    if metadata.last_change.elapsed() < throttle {
                        return true;
                    }
                }
            }
            false
        })?;
        if skip_notification {
            info!("Skip handleTextChanged due to throttling");
            return Ok(());
        }

        self.text_document_did_change(params)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn handle_buf_write_post(&self, params: &Value) -> Result<()> {
        self.text_document_did_save(params)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn handle_buf_delete(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        if !self.get_config(|c| c.server_commands.contains_key(&language_id))? {
            return Ok(());
        }

        self.update_state(|state| {
            state.text_documents.retain(|f, _| f != &filename);
            state.diagnostics.retain(|f, _| f != &filename);
            state.line_diagnostics.retain(|fl, _| fl.0 != *filename);
            Ok(())
        })?;
        self.text_document_did_close(params)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn get_signs_to_display(&self, filename: &str, viewport: &Viewport) -> Result<Vec<Sign>> {
        let max_signs = self.get_config(|c| c.diagnostics_signs_max.unwrap_or(std::usize::MAX))?;
        let signs: Vec<_> = self.get_state(|state| {
            let diagnostics = state.diagnostics.get(filename).cloned().unwrap_or_default();
            let mut diagnostics = diagnostics
                .iter()
                .filter(|diag| viewport.overlaps(diag.range))
                .sorted_by_key(|diag| {
                    (
                        diag.range.start.line,
                        diag.severity.unwrap_or(DiagnosticSeverity::Hint),
                    )
                })
                .collect_vec();
            diagnostics.dedup_by_key(|diag| diag.range.start.line);
            diagnostics
                .into_iter()
                .take(max_signs)
                .map(Into::into)
                .collect()
        })?;

        Ok(signs)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn handle_cursor_moved(&self, params: &Value, force_redraw: bool) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let line = self.vim()?.get_position(params)?.line;
        if !self.get_config(|c| c.server_commands.contains_key(&language_id))? {
            return Ok(());
        }
        if !self.get_state(|state| state.diagnostics.contains_key(&filename))?
            && !self.get_state(|state| state.code_lens.contains_key(&filename))?
        {
            return Ok(());
        }

        if line != self.get_state(|state| state.last_cursor_line)? {
            let message = self.get_state(|state| {
                state
                    .line_diagnostics
                    .get(&(filename.clone(), line))
                    .cloned()
                    .unwrap_or_default()
            })?;

            if message != self.get_state(|state| state.last_line_diagnostic.clone())? {
                self.vim()?.echo_ellipsis(&message)?;
                self.update_state(|state| {
                    state.last_line_diagnostic = message;
                    Ok(())
                })?;
            }

            self.update_state(|state| {
                state.last_cursor_line = line;
                Ok(())
            })?;
        }

        let current_viewport = self.vim()?.get_viewport(params)?;
        let previous_viewport = self.get_state(|state| state.viewports.get(&filename).cloned())?;
        match previous_viewport {
            // if the viewport hasn't changed and force_redraw is not set, we can safely exit this
            // function early and save us some sign and virtual text redrawing.
            Some(pv) if pv == current_viewport && !force_redraw => {
                return Ok(());
            }
            _ => {}
        }

        let signs = self.get_signs_to_display(&filename, &current_viewport)?;
        self.update_state(|state| {
            state.viewports.insert(filename.clone(), current_viewport);
            Ok(())
        })?;
        self.vim()?.set_signs(&filename, &signs)?;

        let highlights: Vec<_> = self.update_state(|state| {
            Ok(state
                .highlights
                .entry(filename.clone())
                .or_insert_with(Vec::new)
                .iter()
                .filter_map(|h| {
                    if h.line < current_viewport.start || h.line > current_viewport.end {
                        return None;
                    }

                    Some(h.clone())
                })
                .collect())
        })?;

        self.vim()?
            .set_highlights(&highlights, "__LCN_DIAGNOSTIC_HIGHLIGHT__")?;
        self.draw_virtual_texts(&params)?;

        Ok(())
    }

    fn draw_virtual_texts(&self, params: &Value) -> Result<()> {
        if !self.get_config(|c| c.is_nvim)? {
            return Ok(());
        }

        let filename = self.vim()?.get_filename(params)?;
        let filename = filename.as_str();
        let viewport = self.vim()?.get_viewport(params)?;
        let bufnr = self.vim()?.get_bufnr(&filename, params)?;
        let namespace_id = self.get_or_create_namespace(&LCNamespace::VirtualText)?;
        let is_insert_mode = self.vim()?.get_mode()? == Mode::Insert;
        if self.get_config(|c| c.hide_virtual_texts_on_insert)? && is_insert_mode {
            self.vim()?.set_virtual_texts(
                bufnr,
                namespace_id,
                viewport.start,
                viewport.end,
                &[],
            )?;
            return Ok(());
        }

        let mut virtual_texts = vec![];
        let use_virtual_text = self.get_config(|c| c.use_virtual_text.clone())?;

        // code lens
        if UseVirtualText::All == use_virtual_text || UseVirtualText::CodeLens == use_virtual_text {
            virtual_texts.extend(
                self.virtual_texts_from_code_lenses(filename, &viewport)?
                    .into_iter(),
            );
        }

        // inlay hints
        if UseVirtualText::All == use_virtual_text || UseVirtualText::CodeLens == use_virtual_text {
            let additional_virtual_texts =
                self.virtual_texts_from_inlay_hints(filename, &viewport)?;
            virtual_texts.extend(additional_virtual_texts);
        }

        // diagnostics
        if UseVirtualText::All == use_virtual_text
            || UseVirtualText::Diagnostics == use_virtual_text
        {
            let vt_diagnostics = self
                .virtual_texts_from_diagnostics(filename, &viewport)?
                .into_iter();
            virtual_texts.extend(vt_diagnostics);
        }

        self.vim()?.set_virtual_texts(
            bufnr,
            namespace_id,
            viewport.start,
            viewport.end,
            &virtual_texts,
        )?;

        Ok(())
    }

    fn virtual_texts_from_diagnostics(
        &self,
        filename: &str,
        viewport: &viewport::Viewport,
    ) -> Result<Vec<VirtualText>> {
        let mut virtual_texts = vec![];
        let diagnostics = self.get_state(|state| state.diagnostics.clone())?;
        let diagnostics_display = self.get_config(|c| c.diagnostics_display.clone())?;
        let diag_list = diagnostics.get(filename);
        if let Some(diag_list) = diag_list {
            for diag in diag_list {
                if viewport.overlaps(diag.range) {
                    let mut explanation = diag.message.clone();
                    if let Some(source) = &diag.source {
                        explanation = format!("{}: {}\n", source, explanation);
                    }
                    virtual_texts.push(VirtualText {
                        line: diag.range.start.line,
                        text: explanation.replace("\n", "  "),
                        hl_group: diagnostics_display
                            .get(&(diag.severity.unwrap_or(DiagnosticSeverity::Hint) as u64))
                            .ok_or_else(|| anyhow!("Failed to get display"))?
                            .virtual_texthl
                            .clone(),
                    });
                }
            }
        }

        Ok(virtual_texts)
    }

    fn virtual_texts_from_inlay_hints(
        &self,
        filename: &str,
        viewport: &viewport::Viewport,
    ) -> Result<Vec<VirtualText>> {
        let inlay_hints: Vec<InlayHint> = self.get_state(|state| {
            state
                .inlay_hints
                .get(filename)
                .map(|s| {
                    s.iter()
                        .filter(|hint| viewport.overlaps(hint.range))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        })?;
        let hl_group = self.get_config(|c| c.code_lens_display.virtual_texthl.clone())?;

        let virtual_texts = inlay_hints
            .into_iter()
            .map(|hint| VirtualText {
                line: hint.range.end.line,
                text: hint.label,
                hl_group: hl_group.clone(),
            })
            .collect();
        Ok(virtual_texts)
    }

    fn virtual_texts_from_code_lenses(
        &self,
        filename: &str,
        viewport: &viewport::Viewport,
    ) -> Result<Vec<VirtualText>> {
        let mut virtual_texts = vec![];
        let code_lenses: Vec<CodeLens> =
            self.get_state(|state| match state.code_lens.get(filename) {
                Some(cls) => cls
                    .into_iter()
                    .filter(|cl| viewport.overlaps(cl.range))
                    .cloned()
                    .collect(),
                None => vec![],
            })?;
        let hl_group = self.get_config(|c| c.code_lens_display.virtual_texthl.clone())?;

        for cl in code_lenses {
            if let Some(command) = cl.command {
                let line = cl.range.start.line;
                let text = command.title;

                match virtual_texts
                    .iter()
                    .position(|v: &VirtualText| v.line == line)
                {
                    Some(idx) => virtual_texts[idx]
                        .text
                        .push_str(format!(" | {}", text).as_str()),
                    None => virtual_texts.push(VirtualText {
                        line,
                        text,
                        hl_group: hl_group.clone(),
                    }),
                }
            }
        }

        Ok(virtual_texts)
    }

    pub fn handle_complete_done(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let position = self.vim()?.get_position(params)?;
        let completed_item: VimCompleteItem = try_get("completed_item", params)?
            .ok_or_else(|| anyhow!("completed_item not found!"))?;

        let user_data = match completed_item.user_data {
            Some(user_data) => user_data,
            _ => return Ok(()),
        };
        let user_data: VimCompleteItemUserData = serde_json::from_str(&user_data)?;
        let lspitem = match user_data.lspitem {
            Some(lspitem) => lspitem,
            _ => return Ok(()),
        };

        let mut edits = vec![];
        if self.get_config(|c| c.completion_prefer_text_edit)? {
            if let Some(CompletionTextEdit::InsertAndReplace(_)) = lspitem.text_edit {
                error!("insert and replace is not supported");
            }

            if let Some(CompletionTextEdit::Edit(edit)) = lspitem.text_edit {
                // The text edit should be at the completion point, and deleting the partial text
                // that the user had typed when the language server provided the completion.
                //
                // We want to tweak the edit so that it instead deletes the completion that we've
                // already inserted.
                //
                // Check that we're not doing anything stupid before going ahead with this.
                let mut edit = edit;
                edit.range.end.character =
                    edit.range.start.character + completed_item.word.len() as u64;
                if edit.range.end != position || edit.range.start.line != edit.range.end.line {
                    return Ok(());
                }
                edits.push(edit);
            }
        }

        if self.get_config(|c| c.apply_completion_text_edits)? {
            if let Some(aedits) = lspitem.additional_text_edits {
                edits.extend(aedits);
            };
        }

        if edits.is_empty() {
            return Ok(());
        }

        let position = self.apply_text_edits(filename, &edits, position)?;
        self.vim()?
            .cursor(position.line + 1, position.character + 1)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn fzf_sink_location(&self, params: &Value) -> Result<()> {
        let params = match params {
            Value::Array(ref arr) => Value::Array(arr.clone()),
            _ => {
                return Err(anyhow!("Expecting array params!"));
            }
        };

        let lines = <Vec<String>>::deserialize(&params)?;
        if lines.is_empty() {
            anyhow!("No selection!");
        }

        let location = lines
            .get(0)
            .ok_or_else(|| anyhow!("Failed to get line! lines: {:?}", lines))?
            .split('\t')
            .next()
            .ok_or_else(|| anyhow!("Failed to parse: {:?}", lines))?;
        let tokens: Vec<_> = location.split_terminator(':').collect();

        let (filename, mut tokens_iter): (String, _) = if tokens.len() > 2 {
            let end_index = tokens.len() - 2;
            let path = tokens[..end_index].join(":");
            let rest_tokens_iter = tokens[end_index..].iter();
            (path, rest_tokens_iter)
        } else {
            (self.vim()?.get_filename(&params)?, tokens.iter())
        };

        let line = tokens_iter
            .next()
            .ok_or_else(|| anyhow!("Failed to get line! tokens: {:?}", tokens))?
            .to_int()?
            - 1;
        let character = tokens_iter
            .next()
            .ok_or_else(|| anyhow!("Failed to get character! tokens: {:?}", tokens))?
            .to_int()?
            - 1;

        self.edit(&None, &filename)?;
        self.vim()?.cursor(line + 1, character + 1)?;

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn fzf_sink_command(&self, params: &Value) -> Result<()> {
        let selection: String =
            try_get("selection", params)?.ok_or_else(|| anyhow!("selection not found!"))?;
        let tokens: Vec<&str> = selection.splitn(2, ": ").collect();
        let kind = tokens
            .get(0)
            .cloned()
            .ok_or_else(|| anyhow!("Failed to get title! tokens: {:?}", tokens))?;
        let title = tokens
            .get(1)
            .cloned()
            .ok_or_else(|| anyhow!("Failed to get kind! tokens: {:?}", tokens))?;
        let actions = self.get_state(|state| state.stashed_code_action_actions.clone())?;
        let idx = actions
            .iter()
            .position(|it| code_action_kind_as_str(&it) == kind && it.title == title);

        match idx {
            Some(idx) => self.handle_code_action_selection(&actions, idx)?,
            None => return Err(anyhow!("Action not stashed, please try again")),
        };

        Ok(())
    }

    pub fn semantic_scopes(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let (scopes, mut scope_mapping) = self.get_state(|state| {
            (
                state
                    .semantic_scopes
                    .get(&language_id)
                    .cloned()
                    .unwrap_or_default(),
                state
                    .semantic_scope_to_hl_group_table
                    .get(&language_id)
                    .cloned()
                    .unwrap_or_default(),
            )
        })?;

        let mut semantic_scopes = Vec::new();

        // If the user has not set up highlighting yet the table does not exist
        if scopes.len() > scope_mapping.len() {
            scope_mapping.resize(scopes.len(), None);
        }

        for (scope, opt_hl_group) in scopes.iter().zip(scope_mapping.iter()) {
            if let Some(hl_group) = opt_hl_group {
                semantic_scopes.push(json!({
                    "scope": scope,
                    "hl_group": hl_group,
                }));
            } else {
                semantic_scopes.push(json!({
                    "scope": scope,
                    "hl_group": "None",
                }));
            }
        }

        Ok(json!(semantic_scopes))
    }

    pub fn semantic_highlight_symbols(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let (opt_scopes, opt_hl_state) = self.get_state(|state| {
            (
                state.semantic_scopes.get(&language_id).cloned(),
                state.semantic_highlights.get(&language_id).cloned(),
            )
        })?;

        if let (Some(scopes), Some(hl_state)) = (opt_scopes, opt_hl_state) {
            let mut symbols = Vec::new();

            for sym in hl_state.symbols {
                for token in sym.tokens.unwrap_or_default() {
                    symbols.push(json!({
                        "line": sym.line as u64,
                        "character_start": token.character as u64,
                        "character_end": token.character as u64 + token.length as u64,
                        "scope": scopes.get(token.scope as usize).cloned().unwrap_or_default()
                    }));
                }
            }

            Ok(json!(symbols))
        } else {
            Ok(json!([]))
        }
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn ncm_refresh(&self, params: &Value) -> Result<Value> {
        let params = NCMRefreshParams::deserialize(params)?;
        let NCMRefreshParams { info, ctx } = params;
        if ctx.typed.is_empty() {
            return Ok(Value::Null);
        }

        let filename = ctx.filepath.clone();
        let line = ctx.lnum - 1;
        let character = ctx.col - 1;

        let result = self.text_document_completion(&json!({
            "languageId": ctx.filetype,
            "filename": filename,
            "line": line,
            "character": character,
            "handle": false,
        }))?;
        let result = <Option<CompletionResponse>>::deserialize(result)?;
        let result = result.unwrap_or_else(|| CompletionResponse::Array(vec![]));
        let is_incomplete = match result {
            CompletionResponse::Array(_) => false,
            CompletionResponse::List(ref list) => list.is_incomplete,
        };
        let matches: Result<Vec<VimCompleteItem>> = match result {
            CompletionResponse::Array(arr) => arr,
            CompletionResponse::List(list) => list.items,
        }
        .iter()
        .map(|item| VimCompleteItem::from_lsp(item, None))
        .collect();
        let matches = matches?;
        self.vim()?.rpcclient.notify(
            "cm#complete",
            json!([info.name, ctx, ctx.startcol, matches, is_incomplete]),
        )?;
        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn ncm2_on_complete(&self, params: &Value) -> Result<Value> {
        let orig_ctx = &params["ctx"];
        let ctx = NCM2Context::deserialize(orig_ctx)?;
        if ctx.typed.is_empty() {
            return Ok(Value::Null);
        }

        let filename = ctx.filepath.clone();
        let line = ctx.lnum - 1;
        let character = ctx.ccol - 1;

        let result = self.text_document_completion(&json!({
                "languageId": ctx.filetype,
                "filename": filename,
                "line": line,
                "character": character,
                "handle": false}));
        let is_incomplete;
        let matches;
        if let Ok(ref value) = result {
            let completion = <Option<CompletionResponse>>::deserialize(value)?;
            let completion = completion.unwrap_or_else(|| CompletionResponse::Array(vec![]));
            is_incomplete = match completion {
                CompletionResponse::List(ref list) => list.is_incomplete,
                _ => false,
            };
            let matches_result: Result<Vec<VimCompleteItem>> = match completion {
                CompletionResponse::Array(arr) => arr,
                CompletionResponse::List(list) => list.items,
            }
            .iter()
            .map(|item| VimCompleteItem::from_lsp(item, None))
            .collect();
            matches = matches_result?;
        } else {
            is_incomplete = true;
            matches = vec![];
        }
        self.vim()?.rpcclient.notify(
            "ncm2#complete",
            json!([orig_ctx, ctx.startccol, matches, is_incomplete]),
        )?;
        result
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn explain_error_at_point(&self, params: &Value) -> Result<Value> {
        let silent_mode: bool = try_get("silent", params)?.unwrap_or_default();
        let filename = self.vim()?.get_filename(params)?;
        let position = self.vim()?.get_position(params)?;
        let diag = self.get_state(|state| {
            state
                .diagnostics
                .get(&filename)
                .ok_or_else(|| anyhow!("No diagnostics found: filename: {}", filename,))?
                .iter()
                .find(|dn| position >= dn.range.start && position < dn.range.end)
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "No diagnostics found: filename: {}, line: {}, character: {}",
                        filename,
                        position.line,
                        position.character
                    )
                })
        })?;

        if silent_mode && diag.is_err() {
            return Ok(Value::Null);
        }
        let diag = diag?;

        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let root =
            self.get_state(|state| state.roots.get(&language_id).cloned().unwrap_or_default())?;
        let root_uri = root.to_url()?;

        let mut explanation = diag.message;
        if let Some(source) = diag.source {
            explanation = format!("{}: {}\n", source, explanation);
        }
        if let Some(related_information) = diag.related_information {
            explanation = format!("{}\n", explanation);
            for ri in related_information {
                let prefix = format!("{}/", root_uri);
                let uri = if ri.location.uri.as_str().starts_with(prefix.as_str()) {
                    // Heuristic: if start of stringified URI matches rootUri, abbreviate it away
                    &ri.location.uri.as_str()[root_uri.as_str().len() + 1..]
                } else {
                    ri.location.uri.as_str()
                };
                if ri.location.uri.scheme() == "file" {
                    explanation = format!(
                        "{}\n{}:{}: {}",
                        explanation,
                        uri,
                        &ri.location.range.start.line + 1,
                        &ri.message
                    );
                } else {
                    // Heuristic: if scheme is not file, don't show line numbers
                    explanation = format!("{}\n{}: {}", explanation, uri, &ri.message);
                }
            }
        }

        self.preview(explanation.as_str(), "__LCNExplainError__")?;
        Ok(Value::Null)
    }

    // Extensions by language servers.
    #[tracing::instrument(level = "info", skip(self))]
    pub fn language_status(&self, params: &Value) -> Result<()> {
        let params = LanguageStatusParams::deserialize(params)?;
        let msg = format!("{} {}", params.typee, params.message);
        self.vim()?.echomsg(&msg)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn rust_handle_begin_build(&self, _params: &Value) -> Result<()> {
        self.vim()?.command(vec![
            format!("let {}=1", VIM_SERVER_STATUS),
            format!("let {}='Rust: build begin'", VIM_SERVER_STATUS_MESSAGE),
        ])?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn rust_handle_diagnostics_begin(&self, _params: &Value) -> Result<()> {
        self.vim()?.command(vec![
            format!("let {}=1", VIM_SERVER_STATUS),
            format!(
                "let {}='Rust: diagnostics begin'",
                VIM_SERVER_STATUS_MESSAGE
            ),
        ])?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn rust_handle_diagnostics_end(&self, _params: &Value) -> Result<()> {
        self.vim()?.command(vec![
            format!("let {}=0", VIM_SERVER_STATUS),
            format!("let {}='Rust: diagnostics end'", VIM_SERVER_STATUS_MESSAGE),
        ])?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn window_progress(&self, params: &Value) -> Result<()> {
        let params = WindowProgressParams::deserialize(params)?;

        let done = params.done.unwrap_or(false);

        let mut buf = "LS: ".to_owned();

        if done {
            buf += "Idle";
        } else {
            // For RLS this can be "Build" or "Diagnostics" or "Indexing".
            buf += params.title.as_ref().map(AsRef::as_ref).unwrap_or("Busy");

            // For RLS this is the crate name, present only if the progress isn't known.
            if let Some(message) = params.message {
                buf += &format!(" ({})", &message);
            }
            // For RLS this is the progress percentage, present only if the it's known.
            if let Some(percentage) = params.percentage {
                buf += &format!(" ({:.1}% done)", percentage);
            }
        }

        self.vim()?.command(vec![
            format!("let {}={}", VIM_SERVER_STATUS, if done { 0 } else { 1 }),
            format!(
                "let {}='{}'",
                VIM_SERVER_STATUS_MESSAGE,
                &escape_single_quote(buf)
            ),
        ])?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn start_server(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let cmdargs: Vec<String> = try_get("cmdargs", params)?.unwrap_or_default();
        let cmdparams = vim_cmd_args_to_value(&cmdargs)?;
        let params = params.combine(&cmdparams);

        // When multiple buffers get opened up concurrently,
        // startServer gets called concurrently.
        // This lock ensures that at most one language server is starting up at a time per
        // languageId.
        // We keep the mutex in scope to satisfy the borrow checker.
        // This ensures that the mutex isn't garbage collected while the MutexGuard is held.
        //
        // - e.g. prevents starting multiple servers with `vim -p`.
        // - This continues to allow distinct language servers to start up concurrently
        //   by languageId (e.g. java and rust)
        // - Revisit this when more than one server is allowed per languageId.
        //   (ensure that the mutex is acquired by what starts the group of servers)
        //
        // TODO: May want to lock other methods that update the list of clients.
        let mutex_for_language_id = self.get_client_update_mutex(Some(language_id.clone()))?;
        let _raii_lock: MutexGuard<()> = mutex_for_language_id.lock().map_err(|err| {
            anyhow!(
                "Failed to lock client creation for languageId {:?}: {:?}",
                language_id,
                err
            )
        })?;

        if self.get_state(|state| state.clients.contains_key(&Some(language_id.clone())))? {
            return Ok(json!({}));
        }

        self.sync_settings()?;
        info!("settings synced");

        let command = self.get_config(|c| {
            c.server_commands.get(&language_id).cloned().ok_or_else(|| {
                Error::from(LCError::NoServerCommands {
                    language_id: language_id.clone(),
                })
            })
        })??;
        let command = command.get_command();

        let root_path: Option<String> = try_get("rootPath", &params)?;
        let root = if let Some(r) = root_path {
            r
        } else {
            get_root_path(
                Path::new(&filename),
                &language_id,
                &self.get_config(|c| c.root_markers.clone())?,
            )?
            .to_string_lossy()
            .into()
        };
        let message = format!("Project root: {}", root);
        if self.get_config(|c| c.echo_project_root)? {
            self.vim()?.echomsg_ellipsis(&message)?;
        }
        info!("{}", message);
        self.update_state(|state| {
            state.roots.insert(language_id.clone(), root.clone());
            Ok(())
        })?;

        let (child_id, reader, writer): (_, Box<dyn SyncRead>, Box<dyn SyncWrite>) =
            if command.get(0).map(|c| c.starts_with("tcp://")) == Some(true) {
                let addr = command
                    .get(0)
                    .map(|s| s.replace("tcp://", ""))
                    .ok_or_else(|| anyhow!("Server command can't be empty!"))?;
                let stream = TcpStream::connect(addr)?;
                let reader = Box::new(BufReader::new(stream.try_clone()?));
                let writer = Box::new(BufWriter::new(stream));
                (None, reader, writer)
            } else {
                let command: Vec<_> = command
                    .into_iter()
                    .map(|cmd| match shellexpand::full(&cmd) {
                        Ok(cmd) => cmd.as_ref().into(),
                        Err(err) => {
                            warn!("Error expanding ({}): {}", cmd, err);
                            cmd.clone()
                        }
                    })
                    .collect();

                let stderr = match self.get_config(|c| c.server_stderr.clone())? {
                    Some(ref path) => std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .with_context(|| format!("Failed to open file ({})", path))?
                        .into(),
                    None => Stdio::null(),
                };

                let process = std::process::Command::new(
                    command.get(0).ok_or_else(|| anyhow!("Empty command!"))?,
                )
                .args(&command[1..])
                .current_dir(&root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(stderr)
                .spawn()
                .with_context(|| format!("Failed to start language server ({:?})", command))?;

                let child_id = Some(process.id());
                let reader = Box::new(BufReader::new(
                    process
                        .stdout
                        .ok_or_else(|| anyhow!("Failed to get subprocess stdout"))?,
                ));
                let writer = Box::new(BufWriter::new(
                    process
                        .stdin
                        .ok_or_else(|| anyhow!("Failed to get subprocess stdin"))?,
                ));
                (child_id, reader, writer)
            };

        let lcn = self.clone();
        let on_server_crash = move |language_id: &LanguageId| {
            if let Err(err) = lcn.on_server_crash(language_id) {
                error!("Restart attempt failed: {}", err);
            }
        };

        let client = RpcClient::new(
            Some(language_id.clone()),
            reader,
            writer,
            child_id,
            self.get_state(|state| state.tx.clone())?,
            on_server_crash,
        )?;
        self.update_state(|state| {
            state
                .clients
                .insert(Some(language_id.clone()), Arc::new(client));
            Ok(())
        })?;

        if self.get_state(|state| state.clients.len())? == 2 {
            self.define_signs()?;
        }

        self.initialize(&params)?;
        self.initialized(&params)?;

        let root =
            self.get_state(|state| state.roots.get(&language_id).cloned().unwrap_or_default())?;
        match self.get_workspace_settings(&root) {
            Ok(Value::Null) => (),
            Ok(settings) => self.workspace_did_change_configuration(&json!({
                "languageId": language_id,
                "settings": settings,
            }))?,
            Err(err) => warn!("Failed to get workspace settings: {}", err),
        }

        self.vim()?
            .rpcclient
            .notify("setbufvar", json!([filename, VIM_IS_SERVER_RUNNING, 1]))?;

        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageClientStarted")?;
        Ok(Value::Null)
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn on_server_crash(&self, language_id: &LanguageId) -> Result<()> {
        if language_id.is_none() {
            return Ok(());
        }

        // we don't want to restart if the server was shut down by the user, so check
        // VIM_IS_SERVER_RUNNING as that should be true at this point only if the server exited
        // unexpectedly.
        let filename = self.vim()?.get_filename(&Value::Null)?;
        let is_running: u8 = self
            .vim()?
            .getbufvar(filename.as_str(), VIM_IS_SERVER_RUNNING)?;
        let is_running = is_running == 1;
        if !is_running {
            return Ok(());
        }

        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageServerCrashed")?;
        self.vim()?
            .rpcclient
            .notify("setbufvar", json!([filename, VIM_IS_SERVER_RUNNING, 0]))?;

        if !self.get_config(|c| c.restart_on_crash)? {
            return Ok(());
        }

        let max_restart_retries = self.get_config(|c| c.max_restart_retries)?;
        let mut restarts =
            self.get_state(|state| state.restarts.get(language_id).cloned().unwrap_or_default())?;
        restarts += 1;

        self.update_state(|state| {
            let mut restarts = restarts;
            if restarts > max_restart_retries {
                restarts = 0;
            };

            state.clients.remove(language_id);
            state.restarts.insert(language_id.clone(), restarts);
            Ok(())
        })?;

        if restarts > max_restart_retries {
            self.vim()?.echoerr(format!(
                "Server for {} restarted too many times, not retrying any more.",
                language_id.clone().unwrap()
            ))?;
            return Ok(());
        }

        self.vim()?.echoerr("Server crashed, restarting client")?;
        std::thread::sleep(Duration::from_millis(300 * (restarts as u64).pow(2)));
        self.start_server(&json!({"languageId": language_id.clone().unwrap()}))?;
        self.text_document_did_open(&json!({
            "languageId": language_id.clone().unwrap(),
            "filename": filename,
        }))?;

        Ok(())
    }

    pub fn handle_server_exited(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let message: String = try_get("message", params)?.unwrap_or_default();

        if self.get_state(|state| state.clients.contains_key(&Some(language_id.clone())))? {
            if let Err(err) = self.cleanup(&language_id) {
                error!("Error in cleanup: {:?}", err);
            }
            if let Err(err) = self.vim()?.echoerr(format!(
                "Language server {} exited unexpectedly: {}",
                language_id, message
            )) {
                error!("Error in echoerr: {:?}", err);
            }
        }

        Ok(())
    }

    pub fn handle_fs_events(&self) -> Result<()> {
        let mut pending_changes = HashMap::new();
        self.update_state(|state| {
            for (language_id, watcher_rx) in &mut state.watcher_rxs {
                let mut events = vec![];
                loop {
                    let result = watcher_rx.try_recv();
                    let event = match result {
                        Ok(event) => event,
                        Err(mpsc::TryRecvError::Empty) => {
                            break;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            return Err(anyhow!("File system notification channel disconnected!"));
                        }
                    };
                    events.push(event);
                }

                let mut changes = vec![];
                for e in events {
                    if let Ok(c) = e.to_lsp() {
                        changes.extend(c);
                    }
                }

                if changes.is_empty() {
                    continue;
                }

                pending_changes.insert(language_id.to_owned(), changes);
            }
            Ok(())
        })?;

        for (language_id, changes) in pending_changes {
            self.workspace_did_change_watched_files(&json!({
                "languageId": language_id,
                "changes": changes
            }))?;
        }

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn workspace_did_change_watched_files(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let params = DidChangeWatchedFilesParams::deserialize(params)?;
        self.get_client(&Some(language_id))?.notify(
            lsp_types::notification::DidChangeWatchedFiles::METHOD,
            params,
        )?;

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn java_class_file_contents(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;

        let content: String = self
            .get_client(&Some(language_id))?
            .call(REQUEST_CLASS_FILE_CONTENTS, params)?;

        let lines: Vec<String> = content
            .lines()
            .map(std::string::ToString::to_string)
            .collect();

        let goto_cmd = self
            .vim()?
            .get_goto_cmd(params)?
            .unwrap_or_else(|| "edit".to_string());

        let uri: String =
            try_get("uri", params)?.ok_or_else(|| anyhow!("uri not found in request!"))?;

        self.vim()?
            .rpcclient
            .notify("s:Edit", json!([goto_cmd, uri]))?;

        self.vim()?.setline(1, &lines)?;
        self.vim()?
            .command("setlocal buftype=nofile filetype=java noswapfile")?;

        Ok(Value::String(content))
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn debug_info(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let server_stderr = self.get_config(|c| c.server_stderr.clone().unwrap_or_default())?;
        let mut msg = String::new();
        self.get_state(|state| {
            msg += &format!(
                "Project root: {}\n",
                state.roots.get(&language_id).cloned().unwrap_or_default()
            );
            msg += &format!(
                "Language server process id: {:?}\n",
                state
                    .clients
                    .get(&Some(language_id.clone()))
                    .map(|c| c.process_id)
                    .unwrap_or_default(),
            );
            msg += &format!("Language server stderr: {}\n", server_stderr,);
            msg += &format!("Log level: {}\n", state.logger.level);
            msg += &format!("Log file: {:?}\n", state.logger.path);
        })?;
        self.vim()?.echo(&msg)?;
        Ok(json!(msg))
    }
}

fn merged_initialization_options(
    command: &ServerCommand,
    settings: &Value,
) -> Result<Option<Value>> {
    let server_name = command.name();
    let section = format!("/{}", server_name);
    let default_initialization_options = get_default_initialization_options(&server_name);
    let server_initialization_options = command.initialization_options();
    let workspace_initialization_options =
        settings.pointer(section.as_str()).unwrap_or(&Value::Null);
    let initialization_options = default_initialization_options
        .combine(&server_initialization_options)
        .combine(workspace_initialization_options);

    if initialization_options.is_null() {
        Ok(None)
    } else {
        Ok(Some(initialization_options))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::{ServerCommand, ServerDetails};

    #[test]
    fn test_expands_initialization_options() {
        let settings = json!({
            "rust-analyzer": {
                "rustfmt": {
                    "overrideCommand": ["rustfmt"],
                },
                "checkOnSave": {
                    "overrideCommand": ["cargo", "check"],
                }
            },
        });
        let command = ServerCommand::Detailed(ServerDetails {
            name: "rust-analyzer".into(),
            command: vec!["rust-analyzer".into()],
            initialization_options: Some(json!({
                "inlayHints.enable": true,
            })),
        });

        let options = merged_initialization_options(&command, &settings)
            .expect("could not get initialization options");
        assert!(options.is_some());
        assert_eq!(
            json!({
                "checkOnSave": {
                    "overrideCommand": ["cargo", "check"],
                },
                "inlayHints": {
                    "enable": true,
                },
                "rustfmt": {
                    "overrideCommand": ["rustfmt"],
                },
            }),
            options.unwrap()
        );
    }

    #[test]
    fn test_handles_empty_global_options() {
        let settings = json!({
            "gopls": {
                "local": "github.com/import/path/to/package"
            }
        });
        let command = ServerCommand::Detailed(ServerDetails {
            name: "gopls".into(),
            command: vec!["gopls".into()],
            initialization_options: None,
        });

        let options = merged_initialization_options(&command, &settings)
            .expect("could not get initialization options");
        assert!(options.is_some());
        assert_eq!(
            json!({
                "local": "github.com/import/path/to/package",
            }),
            options.unwrap()
        );
    }

    #[test]
    fn test_merges_global_and_workspace_local_options() {
        let settings = json!({
            "gopls": {
                "local": "github.com/import/path/to/package"
            }
        });
        let command = ServerCommand::Detailed(ServerDetails {
            name: "gopls".into(),
            command: vec!["gopls".into()],
            initialization_options: Some(json!({
                "usePlaceholders": true,
            })),
        });

        let options = merged_initialization_options(&command, &settings)
            .expect("could not get initialization options");
        assert!(options.is_some());
        assert_eq!(
            json!({
                "usePlaceholders": true,
                "local": "github.com/import/path/to/package",
            }),
            options.unwrap()
        );
    }

    #[test]
    fn test_handles_options_for_simple_commands() {
        let settings = json!({
            "gopls": {
                "local": "github.com/import/path/to/package"
            }
        });
        let command = ServerCommand::Simple(vec!["gopls".into()]);

        let options = merged_initialization_options(&command, &settings)
            .expect("could not get initialization options");
        assert!(options.is_some());
        assert_eq!(
            json!({
                "local": "github.com/import/path/to/package",
            }),
            options.unwrap()
        );
    }
}
