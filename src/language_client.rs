use crate::{
    config::{Config, LoggerConfig, ServerCommand},
    extensions::java,
    lsp::{text_document, workspace},
    rpcclient::RpcClient,
    sign::Sign,
    types::*,
    utils::{
        apply_text_edits, code_action_kind_as_str, diff_value, expand_json_path,
        get_default_initialization_options, get_root_path, vim_cmd_args_to_value, Canonicalize,
        Combine, ToUrl,
    },
    viewport::Viewport,
    vim::{try_get, Highlight, Mode, Vim},
};
use anyhow::{anyhow, Context, Error, Result};
use itertools::Itertools;
use log::*;
use lsp_types::{
    notification::Notification, request::Request, AnnotatedTextEdit, ClientCapabilities,
    ClientInfo, CodeAction, CodeActionClientCapabilities, CodeActionContext, CodeActionKind,
    CodeActionKindLiteralSupport, CodeActionLiteralSupport, CodeActionOrCommand, CodeActionParams,
    CodeActionResponse, CodeLens, CodeLensClientCapabilities, Command,
    CompletionClientCapabilities, CompletionItemCapability, CompletionResponse, CompletionTextEdit,
    Diagnostic, DiagnosticSeverity, DidChangeWatchedFilesClientCapabilities,
    DidChangeWatchedFilesParams, DocumentChangeOperation, DocumentChanges,
    DocumentColorClientCapabilities, GotoCapability, GotoDefinitionResponse,
    HoverClientCapabilities, InitializeParams, InitializeResult, InitializedParams, Location,
    ParameterInformationSettings, PartialResultParams, Position,
    PublishDiagnosticsClientCapabilities, Range, ResourceOp, SemanticTokensClientCapabilities,
    SemanticTokensClientCapabilitiesRequests, SemanticTokensFullOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, SignatureHelpClientCapabilities,
    SignatureInformationSettings, TextDocumentClientCapabilities, TextDocumentIdentifier,
    TextDocumentPositionParams, TextEdit, WorkDoneProgressParams, WorkspaceClientCapabilities,
    WorkspaceEdit,
};
use serde::de::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{read_to_string, File},
    io::{BufRead, BufReader, BufWriter},
    net::TcpStream,
    ops::{Deref, DerefMut},
    path::Path,
    process::Stdio,
    sync::{mpsc, Arc, Mutex, MutexGuard, RwLock},
    thread,
    time::Duration,
};

#[derive(Clone)]
pub struct LanguageClient {
    version: String,
    state_mutex: Arc<Mutex<State>>,
    clients_mutex: Arc<Mutex<HashMap<LanguageId, Arc<Mutex<()>>>>>,
    config: Arc<RwLock<Config>>,
}

impl LanguageClient {
    pub fn vim(&self) -> Result<Vim> {
        self.get_state(|state| state.vim.clone())
    }

    pub fn get_client(&self, language_id: &LanguageId) -> Result<Arc<RpcClient>> {
        self.get_state(|state| state.clients.get(language_id).cloned())?
            .ok_or_else(|| {
                LanguageClientError::ServerNotRunning {
                    language_id: language_id.clone().unwrap_or_default(),
                }
                .into()
            })
    }

    pub fn get_state<T>(&self, f: impl FnOnce(&State) -> T) -> Result<T> {
        Ok(f(self.lock()?.deref()))
    }

    pub fn new(version: impl Into<String>, state: State) -> Self {
        LanguageClient {
            version: version.into(),
            state_mutex: Arc::new(Mutex::new(state)),
            clients_mutex: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(RwLock::new(Config::default())),
        }
    }

    pub fn version(&self) -> String {
        self.version.clone()
    }

    // NOTE: Don't expose this as public.
    // MutexGuard could easily halt the program when one guard is not released immediately after use.
    fn lock(&self) -> Result<MutexGuard<State>> {
        self.state_mutex
            .lock()
            .map_err(|err| anyhow!("Failed to lock state: {:?}", err))
    }

    // This fetches a mutex that is unique to the provided languageId.
    //
    // Here, we return a mutex instead of the mutex guard because we need to satisfy the borrow
    // checker. Otherwise, there is no way to guarantee that the mutex in the hash map wouldn't be
    // garbage collected as a result of another modification updating the hash map, while something was holding the lock
    pub fn get_client_update_mutex(&self, language_id: LanguageId) -> Result<Arc<Mutex<()>>> {
        let map_guard = self.clients_mutex.lock();
        let mut map = map_guard.map_err(|err| {
            anyhow!(
                "Failed to lock client creation for languageId {:?}: {:?}",
                language_id,
                err,
            )
        })?;
        if !map.contains_key(&language_id) {
            map.insert(language_id.clone(), Arc::new(Mutex::new(())));
        }
        let mutex: Arc<Mutex<()>> = map.get(&language_id).unwrap().clone();
        Ok(mutex)
    }

    pub fn get_config<K>(&self, f: impl FnOnce(&Config) -> K) -> Result<K> {
        Ok(f(self
            .config
            .read()
            .map_err(|err| anyhow!("Failed to lock config for reading: {:?}", err))?
            .deref()))
    }

    pub fn update_config<K>(&self, f: impl FnOnce(&mut Config) -> K) -> Result<K> {
        Ok(f(self
            .config
            .write()
            .map_err(|err| anyhow!("Failed to lock config for writing: {:?}", err))?
            .deref_mut()))
    }

    pub fn update_state<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut state = self.lock()?;
        let mut state = state.deref_mut();

        let v = if log_enabled!(log::Level::Debug) {
            let s = serde_json::to_string(&state)?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };

        let result = f(&mut state);

        let next_v = if log_enabled!(log::Level::Debug) {
            let s = serde_json::to_string(&state)?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };

        for (k, (v1, v2)) in diff_value(&v, &next_v, "state") {
            debug!("{}: {} ==> {}", k, v1, v2);
        }
        result
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

    #[tracing::instrument(level = "info", skip(self))]
    fn sync_settings(&self) -> Result<()> {
        let logger_config = LoggerConfig::parse(self.vim()?)?;
        self.update_state(|state| {
            state.logger.update_settings(
                logger_config.logging_level,
                logger_config.logging_file.clone(),
            )
        })?;

        let config = Config::parse(self.vim()?);
        if let Err(ref err) = config {
            log::error!("{}", err);
        }
        let mut config = config?;

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

        let cld = self.get_config(|c| c.code_lens_display.clone())?;
        cmds.push(format!(
            "sign define LanguageClientCodeLens text={} texthl={}",
            cld.sign_text, cld.sign_texthl
        ));

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
                position = self.apply_text_edits(
                    &uri.filepath()?,
                    &edits
                        .iter()
                        .map(|e| lsp_types::OneOf::Left(e.clone()))
                        .collect::<Vec<lsp_types::OneOf<TextEdit, AnnotatedTextEdit>>>(),
                    position,
                )?;
            }
        }
        self.edit(&None, &filename)?;
        self.vim()?
            .cursor(position.line + 1, position.character + 1)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn clear_document_highlight(&self, _params: &Value) -> Result<()> {
        self.vim()?.clear_highlights("__LCN_DOCUMENT_HIGHLIGHT__")
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn apply_text_edits<P: AsRef<Path> + std::fmt::Debug>(
        &self,
        path: P,
        edits: &[lsp_types::OneOf<TextEdit, AnnotatedTextEdit>],
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
        edits.sort_by_key(|edit| match edit {
            lsp_types::OneOf::Left(edit) => (edit.range.start.line, edit.range.start.character),
            lsp_types::OneOf::Right(ae) => (
                ae.text_edit.range.start.line,
                ae.text_edit.range.start.character,
            ),
        });
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

    pub fn update_quickfixlist(&self) -> Result<()> {
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

    pub fn process_diagnostics(&self, filename: &str, diagnostics: &[Diagnostic]) -> Result<()> {
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
            // this needs to be in this locked block so that notifications that arrive too close to
            // each other do not have the chance to cause any race conditions.
            self.update_state(|state| {
                // Clear old highlights.
                let ids = state.highlight_match_ids.clone();
                state.vim.rpcclient.notify("s:MatchDelete", json!([ids]))?;

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

                    let match_id = state
                        .vim
                        .rpcclient
                        .call("matchaddpos", json!([hl_group, ranges]))?;
                    new_match_ids.push(match_id);
                }

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

    pub fn get_line(&self, path: impl AsRef<Path>, line: u32) -> Result<String> {
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
                .nth(line as usize)
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
    pub fn cleanup(&self, language_id: &str) -> Result<()> {
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

    pub fn preview<D>(&self, to_display: &D, bufname: &str) -> Result<()>
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

        let handlers = command.handlers();
        if !handlers.is_empty() {
            self.update_state(|s| {
                s.custom_handlers
                    .entry(language_id.clone())
                    .or_insert_with(HashMap::new)
                    .extend(handlers);
                Ok(())
            })?;
        }

        let settings = self.get_workspace_settings(&root).unwrap_or_default();
        // warn the user that they are using a deprecated workspace settings
        // file format and direct them to the documentation about the new one
        if settings.pointer("/initializationOptions").is_some() {
            let _ = self.vim()?.echoerr("You seem to be using an incorrect workspace settings format for LanguageClient-neovim, to learn more about this error see `:help g:LanguageClient_settingsPath`");
        }

        let initialization_options = merged_initialization_options(&command, &settings);

        let result: Value = self.get_client(&Some(language_id.clone()))?.call(
            lsp_types::request::Initialize::METHOD,
            #[allow(deprecated)]
            InitializeParams {
                client_info: Some(ClientInfo {
                    name: "LanguageClient-neovim".into(),
                    version: Some(self.version()),
                }),
                process_id: Some(std::process::id()),
                /* deprecated in lsp types, but can't initialize without it */
                root_path: Some(root.clone()),
                root_uri: Some(root.to_url()?),
                initialization_options: initialization_options.clone(),
                capabilities: ClientCapabilities {
                    text_document: Some(TextDocumentClientCapabilities {
                        color_provider: Some(DocumentColorClientCapabilities {
                            dynamic_registration: Some(false),
                        }),
                        completion: Some(CompletionClientCapabilities {
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
                            ..CompletionClientCapabilities::default()
                        }),
                        code_action: Some(CodeActionClientCapabilities {
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
                            ..CodeActionClientCapabilities::default()
                        }),
                        signature_help: Some(SignatureHelpClientCapabilities {
                            signature_information: Some(SignatureInformationSettings {
                                active_parameter_support: None,
                                documentation_format: preferred_markup_kind.clone(),
                                parameter_information: Some(ParameterInformationSettings {
                                    label_offset_support: Some(true),
                                }),
                            }),
                            ..SignatureHelpClientCapabilities::default()
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
                        code_lens: Some(CodeLensClientCapabilities {
                            dynamic_registration: Some(true),
                        }),
                        semantic_tokens: Some(SemanticTokensClientCapabilities {
                            dynamic_registration: None,
                            requests: SemanticTokensClientCapabilitiesRequests {
                                range: Some(false),
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                            },
                            token_types: vec![],
                            token_modifiers: vec![],
                            formats: vec![],
                            overlapping_token_support: Some(false),
                            multiline_token_support: Some(false),
                        }),
                        semantic_highlighting_capabilities: None,
                        hover: Some(HoverClientCapabilities {
                            content_format: preferred_markup_kind,
                            ..HoverClientCapabilities::default()
                        }),
                        ..TextDocumentClientCapabilities::default()
                    }),
                    workspace: Some(WorkspaceClientCapabilities {
                        apply_edit: Some(true),
                        configuration: Some(true),
                        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                            dynamic_registration: Some(true),
                        }),
                        ..WorkspaceClientCapabilities::default()
                    }),
                    ..ClientCapabilities::default()
                },
                trace: Some(trace),
                workspace_folders: None,
                locale: None,
            },
        )?;

        let initialize_result = InitializeResult::deserialize(&result)?;
        self.update_state(|state| {
            let server_name = initialize_result
                .server_info
                .as_ref()
                .map(|info| info.name.clone());
            if let (Some(name), Some(options)) = (server_name, initialization_options) {
                state.initialization_options = state
                    .initialization_options
                    .combine(&json!({ name: options }));
            }

            let capabilities: ServerCapabilities = initialize_result.capabilities.clone();
            if let Some(cap) = capabilities.semantic_tokens_provider {
                let legend = match cap {
                    SemanticTokensServerCapabilities::SemanticTokensOptions(c) => c.legend,
                    SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(c) => {
                        c.semantic_tokens_options.legend
                    }
                };
                state
                    .semantic_token_legends
                    .insert(language_id.clone(), legend);
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

        Ok(result)
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn initialized(&self, params: &Value) -> Result<()> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, params)?;
        self.get_client(&Some(language_id))?.notify(
            lsp_types::notification::Initialized::METHOD,
            InitializedParams {},
        )?;
        Ok(())
    }

    /// Generic find locations, e.g, definitions, references.
    #[tracing::instrument(level = "info", skip(self))]
    pub fn find_locations(&self, params: &Value) -> Result<Value> {
        text_document::did_change(self, params)?;
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

    pub fn get_code_actions(&self, params: &Value) -> Result<Value> {
        text_document::did_change(self, params)?;
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
        if actions.is_empty() {
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

    pub fn handle_code_action_selection(&self, actions: &[CodeAction], idx: usize) -> Result<()> {
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
                        workspace::execute_command(self, &params)?;
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

    // shows a list of actions for the user to choose one.
    pub fn present_actions<T, F>(&self, title: &str, actions: &[T], callback: F) -> Result<()>
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
            SelectionUi::Funcref => {
                self.vim()?.rpcclient.notify(
                    "s:selectionUI_funcref",
                    json!([actions?, NOTIFICATION_FZF_SINK_COMMAND]),
                )?;
            }
            SelectionUi::Quickfix | SelectionUi::LocationList => {
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
            SelectionUi::Funcref => {
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
            SelectionUi::Quickfix => {
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
            SelectionUi::LocationList => {
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
        let result = text_document::completion(self, params)?;
        let result = <Option<CompletionResponse>>::deserialize(result)?;
        let result = result.unwrap_or_else(|| CompletionResponse::Array(vec![]));
        let matches = match result {
            CompletionResponse::Array(arr) => arr,
            CompletionResponse::List(list) => list.items,
        };

        let complete_position: Option<u32> = try_get("complete_position", params)?;

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
            text_document::did_open(self, params)?;
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
            text_document::did_open(self, params)?;

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
                text_document::did_open(self, params)?;
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

        text_document::did_change(self, params)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn handle_buf_write_post(&self, params: &Value) -> Result<()> {
        text_document::did_save(self, params)?;
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
        text_document::did_close(self, params)?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn get_signs_to_display(&self, filename: &str, viewport: &Viewport) -> Result<Vec<Sign>> {
        let max_signs = self.get_config(|c| c.diagnostics_signs_max.unwrap_or(std::usize::MAX))?;
        let mut signs = vec![];
        let mut diagnostics: Vec<Sign> = self.get_state(|state| {
            let diagnostics = state.diagnostics.get(filename).cloned().unwrap_or_default();
            let mut diagnostics = diagnostics
                .into_iter()
                .filter(|diag| viewport.overlaps(diag.range))
                .sorted_by_key(|diag| {
                    (
                        diag.range.start.line,
                        diag.severity.unwrap_or(DiagnosticSeverity::Hint),
                    )
                })
                .collect_vec();
            diagnostics.dedup_by_key(|diag| diag.range.start.line);
            diagnostics.iter().map(Into::into).collect_vec()
        })?;

        let mut code_lenses: Vec<Sign> = self.get_state(|state| {
            let ccll = state.code_lens.get(filename).cloned().unwrap_or_default();
            let ccll = ccll
                .iter()
                .filter(|cl| viewport.overlaps(cl.range))
                .map(Into::into)
                .collect_vec();
            ccll
        })?;

        signs.append(&mut diagnostics);
        signs.append(&mut code_lenses);
        Ok(signs.into_iter().take(max_signs).collect())
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

    pub fn draw_virtual_texts(&self, params: &Value) -> Result<()> {
        if !self.get_config(|c| c.is_nvim)? {
            return Ok(());
        }

        let filename = self.vim()?.get_filename(params)?;
        let filename = filename.as_str();
        let viewport = self.vim()?.get_viewport(params)?;
        let bufnr = self.vim()?.get_bufnr(&filename, params)?;
        let namespace_id = self.get_or_create_namespace(&LanguageClientNamespace::VirtualText)?;
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
        viewport: &Viewport,
    ) -> Result<Vec<VirtualText>> {
        let mut virtual_texts = vec![];
        let diagnostics =
            self.get_state(|state| state.diagnostics.get(filename).cloned().unwrap_or_default())?;
        let diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .sorted_by(|a, b| Ord::cmp(&b.severity, &a.severity))
            .collect();
        let diagnostics_display = self.get_config(|c| c.diagnostics_display.clone())?;
        for diag in diagnostics {
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

        Ok(virtual_texts)
    }

    fn virtual_texts_from_inlay_hints(
        &self,
        filename: &str,
        viewport: &Viewport,
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
        viewport: &Viewport,
    ) -> Result<Vec<VirtualText>> {
        let mut virtual_texts = vec![];
        let code_lenses: Vec<CodeLens> =
            self.get_state(|state| match state.code_lens.get(filename) {
                Some(cls) => cls
                    .iter()
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
                    edit.range.start.character + completed_item.word.len() as u32;
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

        let position = self.apply_text_edits(
            filename,
            &edits
                .into_iter()
                .map(lsp_types::OneOf::Left)
                .collect::<Vec<lsp_types::OneOf<TextEdit, AnnotatedTextEdit>>>(),
            position,
        )?;
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

        let lines = <Vec<String>>::deserialize(&params[0])?;
        if lines.is_empty() {
            anyhow!("No selection!");
        }

        let fzf_action: HashMap<String, String> = self.vim()?.eval("s:GetFZFAction()")?;
        let goto_cmd = match lines.get(0) {
            Some(action) if fzf_action.contains_key(action) => fzf_action.get(action).cloned(),
            _ => Some("edit".to_string()),
        };

        let location = lines
            .get(1)
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
            .parse::<u32>()?
            - 1;
        let character = tokens_iter
            .next()
            .ok_or_else(|| anyhow!("Failed to get character! tokens: {:?}", tokens))?
            .parse::<u32>()?
            - 1;

        self.edit(&goto_cmd, &filename)?;
        self.vim()?.cursor(line + 1, character + 1)?;

        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn fzf_sink_command(&self, params: &Value) -> Result<()> {
        let fzf_selection: Vec<String> =
            try_get("selection", params)?.ok_or_else(|| anyhow!("selection not found!"))?;
        let selection = &fzf_selection[1]; // ignore the first element `fzf_action`
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

    #[tracing::instrument(level = "info", skip(self))]
    pub fn ncm_refresh(&self, params: &Value) -> Result<Value> {
        let params = NcmRefreshParams::deserialize(params)?;
        let NcmRefreshParams { info, ctx } = params;
        if ctx.typed.is_empty() {
            return Ok(Value::Null);
        }

        let filename = ctx.filepath.clone();
        let line = ctx.lnum - 1;
        let character = ctx.col - 1;

        let result = text_document::completion(
            self,
            &json!({
                "languageId": ctx.filetype,
                "filename": filename,
                "line": line,
                "character": character,
                "handle": false,
            }),
        )?;
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
        let ctx = Ncm2Context::deserialize(orig_ctx)?;
        if ctx.typed.is_empty() {
            return Ok(Value::Null);
        }

        let filename = ctx.filepath.clone();
        let line = ctx.lnum - 1;
        let character = ctx.ccol - 1;

        let result = text_document::completion(
            self,
            &json!({
                "languageId": ctx.filetype,
                "filename": filename,
                "line": line,
                "character": character,
                "handle": false}),
        );
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
        let diagnostics: Result<Vec<Diagnostic>> = self.get_state(|state| {
            Ok(state
                .diagnostics
                .get(&filename)
                .ok_or_else(|| anyhow!("No diagnostics found: filename: {}", filename,))?
                .iter()
                .filter(|dn| position >= dn.range.start && position <= dn.range.end)
                .cloned()
                .collect::<Vec<Diagnostic>>())
        })?;

        if silent_mode && diagnostics.is_err() {
            return Ok(Value::Null);
        }
        let diagnostics = diagnostics?;

        let language_id = self.vim()?.get_language_id(&filename, params)?;
        let root =
            self.get_state(|state| state.roots.get(&language_id).cloned().unwrap_or_default())?;
        let root_uri = root.to_url()?;

        let mut explanation = vec![];
        for (idx, diagnostic) in diagnostics.iter().enumerate() {
            let mut message = diagnostic.message.clone();
            if let Some(source) = &diagnostic.source {
                message = format!("{}: {}", source, message);
            }
            message = format!("{}. {}", idx + 1, message);

            if let Some(related_information) = &diagnostic.related_information {
                for ri in related_information {
                    let prefix = format!("{}/", root_uri);
                    let uri = if ri.location.uri.as_str().starts_with(prefix.as_str()) {
                        // Heuristic: if start of stringified URI matches rootUri, abbreviate it away
                        &ri.location.uri.as_str()[root_uri.as_str().len() + 1..]
                    } else {
                        ri.location.uri.as_str()
                    };
                    if ri.location.uri.scheme() == "file" {
                        message = format!(
                            "{}\n{}:{}:{}: {}",
                            message,
                            uri,
                            &ri.location.range.start.line + 1,
                            &ri.location.range.start.character + 1,
                            &ri.message
                        );
                    } else {
                        // Heuristic: if scheme is not file, don't show line numbers
                        message = format!("{}\n{}: {}", message, uri, &ri.message);
                    }
                }
            }

            explanation.push(message);
        }

        self.preview(explanation.join("\n").as_str(), "__LCNExplainError__")?;
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
                Error::from(LanguageClientError::NoServerCommands {
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
                    .iter()
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
                // Allocate a much larger buffer size (1 megabyte instead of the BufWriter default of 8kb)
                // so that Vim's ui doesn't get blocked when waiting to write to a language server
                // that is doing work instead of reading from stdin
                // (e.g. if the server is single threaded).
                //
                // On linux, the pipe buffer size defaults to 8 kilobytes.
                // TCP allows much larger buffers than pipe buffers.
                let writer = Box::new(BufWriter::with_capacity(
                    1000000,
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
            Ok(settings) => workspace::did_change_configuration(
                self,
                &json!({
                    "languageId": language_id,
                    "settings": settings,
                }),
            )?,
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
        text_document::did_open(
            self,
            &json!({
                "languageId": language_id.clone().unwrap(),
                "filename": filename,
            }),
        )?;

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

fn merged_initialization_options(command: &ServerCommand, settings: &Value) -> Option<Value> {
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
        None
    } else {
        Some(initialization_options)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::*;

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
            handlers: None,
        });

        let options = merged_initialization_options(&command, &settings);
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
            handlers: None,
        });

        let options = merged_initialization_options(&command, &settings);
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
            handlers: None,
        });

        let options = merged_initialization_options(&command, &settings);
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

        let options = merged_initialization_options(&command, &settings);
        assert!(options.is_some());
        assert_eq!(
            json!({
                "local": "github.com/import/path/to/package",
            }),
            options.unwrap()
        );
    }
}
