use super::*;

use crate::language_client::LanguageClient;
use crate::lsp::notification::Notification;
use crate::lsp::request::GotoDefinitionResponse;
use crate::lsp::request::Request;
use crate::rpcclient::RpcClient;
use crate::sign::Sign;
use failure::err_msg;
use itertools::Itertools;
use notify::Watcher;
use std::sync::mpsc;
use vim::try_get;

impl LanguageClient {
    pub fn get_client(&self, lang_id: &LanguageId) -> Fallible<RpcClient> {
        self.get(|state| state.clients.get(lang_id).cloned())?
            .ok_or_else(|| {
                LCError::ServerNotRunning {
                    languageId: lang_id.clone().unwrap_or_default(),
                }
                .into()
            })
    }

    pub fn loop_call(&self, rx: &crossbeam::channel::Receiver<Call>) -> Fallible<()> {
        for call in rx.iter() {
            let language_client = LanguageClient {
                version: self.version.clone(),
                state_mutex: self.state_mutex.clone(),
                clients_mutex: self.clients_mutex.clone(), // not sure if useful to clone this
            };
            thread::spawn(move || {
                if let Err(err) = language_client.handle_call(call) {
                    error!("Error handling request:\n{:?}", err);
                }
            });
        }

        Ok(())
    }

    /////// Utils ///////
    fn sync_settings(&self) -> Fallible<()> {
        info!("Begin sync settings");
        let (loggingFile, loggingLevel, serverStderr): (
            Option<String>,
            log::LevelFilter,
            Option<String>,
        ) = self.vim()?.eval(
            [
                "get(g:, 'LanguageClient_loggingFile', v:null)",
                "get(g:, 'LanguageClient_loggingLevel', 'WARN')",
                "get(g:, 'LanguageClient_serverStderr', v:null)",
            ]
            .as_ref(),
        )?;
        self.update(|state| logger::update_settings(&state.logger, &loggingFile, loggingLevel))?;

        #[allow(clippy::type_complexity)]
        let (
            autoStart,
            serverCommands,
            selectionUI,
            trace,
            settingsPath,
            loadSettings,
            rootMarkers,
            change_throttle,
            wait_output_timeout,
            diagnosticsEnable,
            diagnosticsList,
            diagnosticsDisplay,
            windowLogMessageLevel,
            hoverPreview,
            completionPreferTextEdit,
            is_nvim,
        ): (
            u64,
            HashMap<String, Vec<String>>,
            Option<String>,
            Option<String>,
            String,
            u64,
            Option<RootMarkers>,
            Option<f64>,
            Option<f64>,
            u64,
            Option<String>,
            Value,
            String,
            Option<String>,
            u64,
            u64,
        ) = self.vim()?.eval(
            [
                "!!get(g:, 'LanguageClient_autoStart', 1)",
                "s:GetVar('LanguageClient_serverCommands', {})",
                "get(g:, 'LanguageClient_selectionUI', v:null)",
                "get(g:, 'LanguageClient_trace', v:null)",
                "expand(get(g:, 'LanguageClient_settingsPath', '.vim/settings.json'))",
                "!!get(g:, 'LanguageClient_loadSettings', 1)",
                "get(g:, 'LanguageClient_rootMarkers', v:null)",
                "get(g:, 'LanguageClient_changeThrottle', v:null)",
                "get(g:, 'LanguageClient_waitOutputTimeout', v:null)",
                "!!get(g:, 'LanguageClient_diagnosticsEnable', 1)",
                "get(g:, 'LanguageClient_diagnosticsList', 'Quickfix')",
                "get(g:, 'LanguageClient_diagnosticsDisplay', {})",
                "get(g:, 'LanguageClient_windowLogMessageLevel', 'Warning')",
                "get(g:, 'LanguageClient_hoverPreview', 'Auto')",
                "get(g:, 'LanguageClient_completionPreferTextEdit', 0)",
                "has('nvim')",
            ]
            .as_ref(),
        )?;

        #[allow(clippy::type_complexity)]
        let (
            diagnosticsSignsMax,
            diagnostics_max_severity,
            documentHighlightDisplay,
            selectionUI_autoOpen,
            use_virtual_text,
            echo_project_root,
            semanticHighlightMaps,
            semanticScopeSeparator,
            applyCompletionAdditionalTextEdits,
            preferred_markup_kind,
        ): (
            Option<u64>,
            String,
            Value,
            u8,
            UseVirtualText,
            u8,
            HashMap<String, HashMap<String, String>>,
            String,
            u8,
            Option<Vec<MarkupKind>>,
        ) = self.vim()?.eval(
            [
                "get(g:, 'LanguageClient_diagnosticsSignsMax', v:null)",
                "get(g:, 'LanguageClient_diagnosticsMaxSeverity', 'Hint')",
                "get(g:, 'LanguageClient_documentHighlightDisplay', {})",
                "!!s:GetVar('LanguageClient_selectionUI_autoOpen', 1)",
                "s:useVirtualText()",
                "!!s:GetVar('LanguageClient_echoProjectRoot', 1)",
                "s:GetVar('LanguageClient_semanticHighlightMaps', {})",
                "s:GetVar('LanguageClient_semanticScopeSeparator', ':')",
                "get(g:, 'LanguageClient_applyCompletionAdditionalTextEdits', 1)",
                "get(g:, 'LanguageClient_preferredMarkupKind', v:null)",
            ]
            .as_ref(),
        )?;

        // vimscript use 1 for true, 0 for false.
        let autoStart = autoStart == 1;
        let selectionUI_autoOpen = selectionUI_autoOpen == 1;
        let loadSettings = loadSettings == 1;

        let trace = if let Some(t) = trace {
            match t.to_ascii_uppercase().as_str() {
                "OFF" => Some(TraceOption::Off),
                "MESSAGES" => Some(TraceOption::Messages),
                "VERBOSE" => Some(TraceOption::Verbose),
                _ => bail!("Invalid option for LanguageClient_trace: {}", t),
            }
        } else {
            Some(TraceOption::default())
        };

        let selectionUI = if let Some(s) = selectionUI {
            SelectionUI::from_str(&s)?
        } else if self.vim()?.eval::<_, i64>("get(g:, 'loaded_fzf')")? == 1 {
            SelectionUI::FZF
        } else {
            SelectionUI::default()
        };

        let change_throttle = change_throttle.map(|t| Duration::from_millis((t * 1000.0) as u64));
        let wait_output_timeout =
            Duration::from_millis((wait_output_timeout.unwrap_or(10.0) * 1000.0) as u64);

        let diagnosticsEnable = diagnosticsEnable == 1;

        let diagnosticsList = if let Some(s) = diagnosticsList {
            DiagnosticsList::from_str(&s)?
        } else {
            DiagnosticsList::Disabled
        };

        let windowLogMessageLevel = match windowLogMessageLevel.to_ascii_uppercase().as_str() {
            "ERROR" => MessageType::Error,
            "WARNING" => MessageType::Warning,
            "INFO" => MessageType::Info,
            "LOG" => MessageType::Log,
            _ => bail!(
                "Invalid option for LanguageClient_windowLogMessageLevel: {}",
                windowLogMessageLevel
            ),
        };

        let hoverPreview = if let Some(s) = hoverPreview {
            HoverPreviewOption::from_str(&s)?
        } else {
            HoverPreviewOption::Auto
        };

        let completionPreferTextEdit = completionPreferTextEdit == 1;
        let applyCompletionAdditionalTextEdits = applyCompletionAdditionalTextEdits == 1;

        let is_nvim = is_nvim == 1;

        let diagnostics_max_severity = match diagnostics_max_severity.to_ascii_uppercase().as_str()
        {
            "ERROR" => DiagnosticSeverity::Error,
            "WARNING" => DiagnosticSeverity::Warning,
            "INFORMATION" => DiagnosticSeverity::Information,
            "HINT" => DiagnosticSeverity::Hint,
            _ => bail!(
                "Invalid option for LanguageClient_diagnosticsMaxSeverity: {}",
                diagnostics_max_severity
            ),
        };

        let semanticHlUpdateLanguageIds: Vec<String> =
            semanticHighlightMaps.keys().cloned().collect();

        self.update(|state| {
            state.autoStart = autoStart;
            state.semanticHighlightMaps = semanticHighlightMaps;
            state.semanticScopeSeparator = semanticScopeSeparator;
            state.semantic_scope_to_hl_group_table.clear();
            state.serverCommands.extend(serverCommands);
            state.selectionUI = selectionUI;
            state.selectionUI_autoOpen = selectionUI_autoOpen;
            state.trace = trace;
            state.diagnosticsEnable = diagnosticsEnable;
            state.diagnosticsList = diagnosticsList;
            state.diagnosticsDisplay = serde_json::from_value(
                serde_json::to_value(&state.diagnosticsDisplay)?.combine(&diagnosticsDisplay),
            )?;
            state.diagnosticsSignsMax = diagnosticsSignsMax;
            state.diagnostics_max_severity = diagnostics_max_severity;
            state.documentHighlightDisplay = serde_json::from_value(
                serde_json::to_value(&state.documentHighlightDisplay)?
                    .combine(&documentHighlightDisplay),
            )?;
            state.windowLogMessageLevel = windowLogMessageLevel;
            state.settingsPath = settingsPath;
            state.loadSettings = loadSettings;
            state.rootMarkers = rootMarkers;
            state.change_throttle = change_throttle;
            state.wait_output_timeout = wait_output_timeout;
            state.hoverPreview = hoverPreview;
            state.completionPreferTextEdit = completionPreferTextEdit;
            state.applyCompletionAdditionalTextEdits = applyCompletionAdditionalTextEdits;
            state.use_virtual_text = use_virtual_text;
            state.echo_project_root = echo_project_root == 1;
            state.loggingFile = loggingFile;
            state.loggingLevel = loggingLevel;
            state.serverStderr = serverStderr;
            state.is_nvim = is_nvim;
            state.preferred_markup_kind = preferred_markup_kind;
            Ok(())
        })?;

        for languageId in semanticHlUpdateLanguageIds {
            self.updateSemanticHighlightTables(&languageId)?;
        }

        info!("End sync settings");
        Ok(())
    }

    fn get_workspace_settings(&self, root: &str) -> Fallible<Value> {
        if !self.get(|state| state.loadSettings)? {
            return Ok(Value::Null);
        }

        let path = Path::new(root).join(&self.get(|state| state.settingsPath.clone())?);
        let buffer = read_to_string(&path).with_context(|err| {
            format!("Failed to read file ({}): {}", path.to_string_lossy(), err)
        })?;
        let value = serde_json::from_str(&buffer)?;
        let value = expand_json_path(value);
        Ok(value)
    }

    fn define_signs(&self) -> Fallible<()> {
        info!("Defining signs");

        let mut cmds = vec![];
        for entry in self.get(|state| state.diagnosticsDisplay.clone())?.values() {
            cmds.push(format!(
                "sign define LanguageClient{} text={} texthl={}",
                entry.name, entry.signText, entry.signTexthl,
            ));
        }

        self.vim()?.command(cmds)?;
        Ok(())
    }

    fn apply_WorkspaceEdit(&self, edit: &WorkspaceEdit) -> Fallible<()> {
        use self::{DocumentChangeOperation::*, ResourceOp::*};

        debug!("Begin apply WorkspaceEdit: {:?}", edit);
        let mut filename = self.vim()?.get_filename(&Value::Null)?;
        let mut position = self.vim()?.get_position(&Value::Null)?;

        if let Some(ref changes) = edit.document_changes {
            match changes {
                DocumentChanges::Edits(ref changes) => {
                    for e in changes {
                        position = self.apply_TextEdits(
                            &e.text_document.uri.filepath()?,
                            &e.edits,
                            position,
                        )?;
                    }
                }
                DocumentChanges::Operations(ref ops) => {
                    for op in ops {
                        match op {
                            Edit(ref e) => {
                                position = self.apply_TextEdits(
                                    &e.text_document.uri.filepath()?,
                                    &e.edits,
                                    position,
                                )?
                            }
                            Op(ref rop) => match rop {
                                Create(file) => {
                                    filename = file.uri.filepath()?.to_string_lossy().into_owned();
                                    position = Position::default();
                                }
                                Rename(_file) => bail!("file renaming not yet supported."),
                                Delete(_file) => bail!("file deletion not yet supported."),
                            },
                        }
                    }
                }
            }
        } else if let Some(ref changes) = edit.changes {
            for (uri, edits) in changes {
                position = self.apply_TextEdits(&uri.filepath()?, edits, position)?;
            }
        }
        self.edit(&None, &filename)?;
        self.vim()?
            .cursor(position.line + 1, position.character + 1)?;
        debug!("End apply WorkspaceEdit");
        Ok(())
    }

    pub fn textDocument_documentHighlight(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::DocumentHighlightRequest::METHOD);
        let filename = self.vim()?.get_filename(&Value::Null)?;
        let languageId = self.vim()?.get_languageId(&filename, &Value::Null)?;
        let position = self.vim()?.get_position(&Value::Null)?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::DocumentHighlightRequest::METHOD,
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

        let document_highlight: Option<Vec<DocumentHighlight>> =
            serde_json::from_value(result.clone())?;
        if let Some(document_highlight) = document_highlight {
            let documentHighlightDisplay =
                self.get(|state| state.documentHighlightDisplay.clone())?;
            let highlights = document_highlight
                .into_iter()
                .map(|DocumentHighlight { range, kind }| {
                    Ok(Highlight {
                        line: range.start.line,
                        character_start: range.start.character,
                        character_end: range.end.character,
                        group: documentHighlightDisplay
                            .get(
                                &kind
                                    .unwrap_or(DocumentHighlightKind::Text)
                                    .to_int()
                                    .unwrap(),
                            )
                            .ok_or_else(|| err_msg("Failed to get display"))?
                            .texthl
                            .clone(),
                        text: String::new(),
                    })
                })
                .collect::<Fallible<Vec<_>>>()?;

            let buffer = self.vim()?.get_bufnr(&filename, params)?;

            // The following code needs to be inside the critical section as a whole to update
            // everything correctly and not leave hanging highlights.
            self.update(|state| {
                let source = if let Some(hs) = state.document_highlight_source {
                    if hs.buffer == buffer {
                        // If we want to highlight in the same buffer as last time, we can reuse
                        // the previous source.
                        Some(hs.source)
                    } else {
                        // Clear the highlight in the previous buffer.
                        state.vim.rpcclient.notify(
                            "nvim_buf_clear_highlight",
                            json!([hs.buffer, hs.source, 0, -1]),
                        )?;

                        None
                    }
                } else {
                    None
                };

                let source = match source {
                    Some(source) => source,
                    None => {
                        // Create a new source.
                        let source = state.vim.rpcclient.call(
                            "nvim_buf_add_highlight",
                            json!([buffer, 0, "Error", 1, 1, 1]),
                        )?;
                        state.document_highlight_source = Some(HighlightSource { buffer, source });
                        source
                    }
                };

                state
                    .vim
                    .rpcclient
                    .notify("nvim_buf_clear_highlight", json!([buffer, source, 0, -1]))?;
                state
                    .vim
                    .rpcclient
                    .notify("s:AddHighlights", json!([source, highlights]))?;

                Ok(())
            })?;
        }

        info!("End {}", lsp::request::DocumentHighlightRequest::METHOD);
        Ok(result)
    }

    pub fn languageClient_clearDocumentHighlight(&self, _params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__ClearDocumentHighlight);

        // The following code needs to be inside the critical section as a whole to update
        // everything correctly and not leave hanging highlights.
        self.update(|state| {
            if let Some(HighlightSource { buffer, source }) = state.document_highlight_source.take()
            {
                state
                    .vim
                    .rpcclient
                    .notify("nvim_buf_clear_highlight", json!([buffer, source, 0, -1]))?;
            }
            Ok(())
        })?;

        info!("End {}", NOTIFICATION__ClearDocumentHighlight);
        Ok(())
    }

    fn apply_TextEdits<P: AsRef<Path>>(
        &self,
        path: P,
        edits: &[TextEdit],
        position: Position,
    ) -> Fallible<Position> {
        debug!("Begin apply TextEdits: {:?}", edits);
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

        let (mut lines, position) = apply_TextEdits(&lines, &edits, &position)?;

        if lines.last().map(String::is_empty) == Some(true) && fixendofline {
            lines.pop();
        }
        if lines.len() < lines_len_prev {
            self.vim()?
                .command(format!("{},{}d", lines.len() + 1, lines_len_prev))?;
        }
        self.vim()?.rpcclient.notify("setline", json!([1, lines]))?;
        debug!("End apply TextEdits");
        Ok(position)
    }

    fn update_quickfixlist(&self) -> Fallible<()> {
        let diagnostics = self.get(|state| state.diagnostics.clone())?;
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
        let diagnosticsList = self.get(|state| state.diagnosticsList)?;
        match diagnosticsList {
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

    fn process_diagnostics(&self, filename: &str, diagnostics: &[Diagnostic]) -> Fallible<()> {
        if !self.get(|state| state.text_documents.contains_key(filename))? {
            return Ok(());
        }

        let text = self.get(|state| {
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
        self.update(|state| {
            state
                .line_diagnostics
                .retain(|&(ref f, _), _| f != filename);
            state.line_diagnostics.extend(line_diagnostics);
            Ok(())
        })?;

        // Highlight.
        let diagnosticsDisplay = self.get(|state| state.diagnosticsDisplay.clone())?;

        let mut highlights = vec![];
        for dn in diagnostics {
            let line = dn.range.start.line;
            let character_start = dn.range.start.character;
            let character_end = dn.range.end.character;

            let severity = dn.severity.unwrap_or(DiagnosticSeverity::Hint);
            let group = diagnosticsDisplay
                .get(&severity.to_int()?)
                .ok_or_else(|| err_msg("Failed to get display"))?
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
        self.update(|state| {
            state.highlights.insert(filename.to_owned(), highlights);
            Ok(())
        })?;

        if !self.get(|state| state.is_nvim)? {
            // Clear old highlights.
            let ids = self.get(|state| state.highlight_match_ids.clone())?;
            self.vim()?
                .rpcclient
                .notify("s:MatchDelete", json!([ids]))?;

            // Group diagnostics by severity so we can highlight them
            // in a single call.
            let mut match_groups: HashMap<_, Vec<_>> = HashMap::new();

            for dn in diagnostics {
                let severity = dn
                    .severity
                    .unwrap_or(DiagnosticSeverity::Information)
                    .to_int()?;
                match_groups
                    .entry(severity)
                    .or_insert_with(Vec::new)
                    .push(dn);
            }

            let mut new_match_ids = Vec::new();

            for (severity, dns) in match_groups {
                let hl_group = diagnosticsDisplay
                    .get(&severity)
                    .ok_or_else(|| err_msg("Failed to get display"))?
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
                            let mut middleLines: Vec<_> = (dn.range.start.line + 1
                                ..dn.range.end.line)
                                .map(|l| vec![l + 1])
                                .collect();
                            let startLine = vec![
                                dn.range.start.line + 1,
                                dn.range.start.character + 1,
                                999_999, //Clear to the end of the line
                            ];
                            let endLine =
                                vec![dn.range.end.line + 1, 1, dn.range.end.character + 1];
                            middleLines.push(startLine);
                            // For a multi-ringe range ending at the exact start of the last line,
                            // don't highlight the first character of the last line.
                            if dn.range.end.character > 0 {
                                middleLines.push(endLine);
                            }
                            middleLines
                        }
                    })
                    .collect();

                let match_id = self
                    .vim()?
                    .rpcclient
                    .call("matchaddpos", json!([hl_group, ranges]))?;
                new_match_ids.push(match_id);
            }
            self.update(|state| {
                state.highlight_match_ids = new_match_ids;
                Ok(())
            })?;
        }

        Ok(())
    }

    fn display_locations(&self, locations: &[Location], title: &str) -> Fallible<()> {
        let location_to_quickfix_entry =
            |state: &Self, loc: &Location| -> Fallible<QuickfixEntry> {
                let filename = loc.uri.filepath()?.to_string_lossy().into_owned();
                let start = loc.range.start;
                let text = state.get_line(&filename, start.line).unwrap_or_default();

                Ok(QuickfixEntry {
                    filename,
                    lnum: start.line + 1,
                    col: Some(start.character + 1),
                    text: Some(text),
                    nr: None,
                    typ: None,
                })
            };

        let selectionUI = self.get(|state| state.selectionUI)?;
        let selectionUI_autoOpen = self.get(|state| state.selectionUI_autoOpen)?;
        match selectionUI {
            SelectionUI::FZF => {
                let cwd: String = self.vim()?.eval("getcwd()")?;
                let source: Fallible<Vec<_>> = locations
                    .iter()
                    .map(|loc| {
                        let filename = loc.uri.filepath()?;
                        let start = loc.range.start;
                        let text = self.get_line(&filename, start.line).unwrap_or_default();
                        let relpath = diff_paths(&filename, Path::new(&cwd)).unwrap_or(filename);
                        Ok(format!(
                            "{}:{}:{}:\t{}",
                            relpath.to_string_lossy(),
                            start.line + 1,
                            start.character + 1,
                            text
                        ))
                    })
                    .collect();
                let source = source?;

                self.vim()?.rpcclient.notify(
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Fallible<Vec<_>> = locations
                    .iter()
                    .map(|loc| location_to_quickfix_entry(self, loc))
                    .collect();
                let list = list?;
                self.vim()?.setqflist(&list, " ", title)?;
                if selectionUI_autoOpen {
                    self.vim()?.command("botright copen")?;
                }
                self.vim()?.echo("Quickfix list updated.")?;
            }
            SelectionUI::LocationList => {
                let list: Fallible<Vec<_>> = locations
                    .iter()
                    .map(|loc| location_to_quickfix_entry(self, loc))
                    .collect();
                let list = list?;
                self.vim()?.setloclist(&list, " ", title)?;
                if selectionUI_autoOpen {
                    self.vim()?.command("lopen")?;
                }
                self.vim()?.echo("Location list updated.")?;
            }
        }
        Ok(())
    }

    fn registerCMSource(&self, languageId: &str, result: &Value) -> Fallible<()> {
        info!("Begin register NCM source");
        let exists_CMRegister: u64 = self.vim()?.eval("exists('g:cm_matcher')")?;
        if exists_CMRegister == 0 {
            return Ok(());
        }

        let result: InitializeResult = serde_json::from_value(result.clone())?;
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
                "name": format!("LanguageClient_{}", languageId),
                "priority": 9,
                "scopes": [languageId],
                "cm_refresh_patterns": trigger_patterns,
                "abbreviation": "LC",
                "cm_refresh": REQUEST__NCMRefresh,
            }]),
        )?;
        info!("End register NCM source");
        Ok(())
    }

    fn registerNCM2Source(&self, languageId: &str, result: &Value) -> Fallible<()> {
        info!("Begin register NCM2 source");
        let exists_ncm2: u64 = self.vim()?.eval("exists('g:ncm2_loaded')")?;
        if exists_ncm2 == 0 {
            return Ok(());
        }

        let result: InitializeResult = serde_json::from_value(result.clone())?;
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
                "name": format!("LanguageClient_{}", languageId),
                "priority": 9,
                "scope": [languageId],
                "complete_pattern": trigger_patterns,
                "mark": "LC",
                "on_complete": REQUEST__NCM2OnComplete,
            }]),
        )?;
        info!("End register NCM2 source");
        Ok(())
    }

    fn parseSemanticScopes(&self, languageId: &str, result: &Value) -> Fallible<()> {
        info!("Begin parse Semantic Scopes");
        let result: InitializeResult = serde_json::from_value(result.clone())?;

        if let Some(capability) = result.capabilities.semantic_highlighting {
            self.update(|state| {
                state
                    .semantic_scopes
                    .insert(languageId.into(), capability.scopes.unwrap_or_default());
                Ok(())
            })?;
        }

        info!("End parse Semantic Scopes");
        Ok(())
    }

    /// Build the Semantic Highlight Lookup Table of
    ///
    /// ScopeIndex -> Option<HighlightGroup>
    fn updateSemanticHighlightTables(&self, languageId: &str) -> Fallible<()> {
        info!("Begin updateSemanticHighlightTables");
        let (opt_scopes, opt_hl_map, scopeSeparator) = self.get(|state| {
            (
                state.semantic_scopes.get(languageId).cloned(),
                state.semanticHighlightMaps.get(languageId).cloned(),
                state.semanticScopeSeparator.clone(),
            )
        })?;

        if let (Some(semantic_scopes), Some(semanticHighlightMap)) = (opt_scopes, opt_hl_map) {
            let mut table: Vec<Option<String>> = Vec::new();

            for scope_list in semantic_scopes {
                // Combine all scopes ["scopeA", "scopeB", ...] -> "scopeA:scopeB:..."
                let scope_str = scope_list.iter().join(&scopeSeparator);

                let mut matched = false;
                for (scope_regex, hl_group) in &semanticHighlightMap {
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

            self.update(|state| {
                state
                    .semantic_scope_to_hl_group_table
                    .insert(languageId.into(), table);
                Ok(())
            })?;
        } else {
            self.update(|state| {
                state.semantic_scope_to_hl_group_table.remove(languageId);
                Ok(())
            })?;
        }
        info!("End updateSemanticHighlightTables");
        Ok(())
    }

    fn get_line(&self, path: impl AsRef<Path>, line: u64) -> Fallible<String> {
        let value = self.vim()?.rpcclient.call(
            "getbufline",
            json!([path.as_ref().to_string_lossy(), line + 1]),
        )?;
        let mut texts: Vec<String> = serde_json::from_value(value)?;
        let mut text = texts.pop().unwrap_or_default();

        if text.is_empty() {
            let reader = BufReader::new(File::open(path)?);
            text = reader
                .lines()
                .nth(line.to_usize()?)
                .ok_or_else(|| format_err!("Failed to get line! line: {}", line))??;
        }

        Ok(text.trim().into())
    }

    fn try_handle_command_by_client(&self, cmd: &Command) -> Fallible<bool> {
        match cmd.command.as_str() {
            "java.apply.workspaceEdit" => {
                if let Some(ref edits) = cmd.arguments {
                    for edit in edits {
                        let edit: WorkspaceEdit = serde_json::from_value(edit.clone())?;
                        self.apply_WorkspaceEdit(&edit)?;
                    }
                }
            }
            "rust-analyzer.showReferences" => {
                let locations = cmd
                    .arguments
                    .clone()
                    .unwrap_or_else(|| vec![])
                    .get(2)
                    .cloned()
                    .unwrap_or_else(|| Value::Array(vec![]));
                let locations: Vec<Location> = serde_json::from_value(locations)?;

                self.display_locations(&locations, "References")?;
            }
            "rust-analyzer.selectAndApplySourceChange" => {
                if let Some(ref edits) = cmd.arguments {
                    for edit in edits {
                        let workspace_edits: Vec<WorkspaceEditWithCursor> =
                            serde_json::from_value(edit.clone())?;
                        for edit in workspace_edits {
                            self.apply_WorkspaceEdit(&edit.workspaceEdit)?;
                            if let Some(cursorPosition) = edit.cursorPosition {
                                self.vim()?.cursor(
                                    cursorPosition.position.line + 1,
                                    cursorPosition.position.character + 1,
                                )?;
                            }
                        }
                    }
                }
            }
            "rust-analyzer.applySourceChange" => {
                if let Some(ref edits) = cmd.arguments {
                    for edit in edits {
                        let edit: WorkspaceEditWithCursor = serde_json::from_value(edit.clone())?;
                        self.apply_WorkspaceEdit(&edit.workspaceEdit)?;
                        if let Some(cursorPosition) = edit.cursorPosition {
                            self.vim()?.cursor(
                                cursorPosition.position.line + 1,
                                cursorPosition.position.character + 1,
                            )?;
                        }
                    }
                }
            }
            "rust-analyzer.runSingle" | "rust-analyzer.run" => {
                let has_term: i32 = self.vim()?.eval("exists(':terminal')")?;
                if has_term == 0 {
                    bail!("Terminal support is required for this action");
                }

                if let Some(ref args) = cmd.arguments {
                    if let Some(args) = args.first().cloned() {
                        let bin: String =
                            try_get("bin", &args)?.ok_or_else(|| err_msg("no bin found"))?;
                        let arguments: Vec<String> = try_get("args", &args)?.unwrap_or_default();
                        let cmd = format!("term {} {}", bin, arguments.join(" "));
                        let cmd = cmd.replace('"', "");
                        self.vim()?.command(cmd)?;
                    }
                }
            }
            // TODO: implement all other rust-analyzer actions
            _ => return Ok(false),
        }

        Ok(true)
    }

    fn cleanup(&self, languageId: &str) -> Fallible<()> {
        info!("Begin cleanup");

        let root = self.get(|state| {
            state
                .roots
                .get(languageId)
                .cloned()
                .ok_or_else(|| format_err!("No project root found! languageId: {}", languageId))
        })??;

        let mut filenames = vec![];
        self.update(|state| {
            for (f, diag_list) in state.diagnostics.iter_mut() {
                if f.starts_with(&root) {
                    filenames.push(f.clone());
                    diag_list.clear();
                }
            }
            Ok(())
        })?;
        for f in filenames {
            self.process_diagnostics(&f, &[])?;
        }
        self.languageClient_handleCursorMoved(&Value::Null)?;

        self.update(|state| {
            state.clients.remove(&Some(languageId.into()));
            state.last_cursor_line = 0;
            state.text_documents.retain(|f, _| !f.starts_with(&root));
            state.roots.remove(languageId);
            Ok(())
        })?;
        self.update_quickfixlist()?;

        self.vim()?.command(vec![
            format!("let {}=0", VIM__ServerStatus),
            format!("let {}=''", VIM__ServerStatusMessage),
        ])?;
        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageClientStopped")?;

        info!("End cleanup");
        Ok(())
    }

    fn preview<D>(&self, to_display: &D) -> Fallible<()>
    where
        D: ToDisplay + ?Sized,
    {
        let bufname = "__LanguageClient__";
        let filetype = &to_display.vim_filetype();
        let lines = to_display.to_display();

        self.vim()?
            .rpcclient
            .notify("s:OpenHoverPreview", json!([bufname, lines, filetype]))?;

        Ok(())
    }

    fn edit(&self, goto_cmd: &Option<String>, path: impl AsRef<Path>) -> Fallible<()> {
        let path = path.as_ref().to_string_lossy();
        if path.starts_with("jdt://") {
            self.java_classFileContents(&json!({ "gotoCmd": goto_cmd, "uri": path }))?;
            Ok(())
        } else {
            self.vim()?.edit(&goto_cmd, path.into_owned())
        }
    }

    /////// LSP ///////

    fn initialize(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::Initialize::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let has_snippet_support: i8 = try_get("hasSnippetSupport", params)?
            .map_or_else(|| self.vim()?.eval("s:hasSnippetSupport()"), Ok)?;
        let has_snippet_support = has_snippet_support > 0;
        let root = self.get(|state| state.roots.get(&languageId).cloned().unwrap_or_default())?;

        let initialization_options = self
            .get_workspace_settings(&root)
            .map(|s| s["initializationOptions"].clone())
            .unwrap_or_else(|err| {
                warn!("Failed to get initializationOptions: {}", err);
                json!(Value::Null)
            });
        let initialization_options =
            get_default_initializationOptions(&languageId).combine(&initialization_options);
        let initialization_options = if initialization_options.is_null() {
            None
        } else {
            Some(initialization_options)
        };

        let trace = self.get(|state| state.trace)?;
        let preferred_markup_kind = self.get(|state| state.preferred_markup_kind.clone())?;

        let result: Value = self.get_client(&Some(languageId.clone()))?.call(
            lsp::request::Initialize::METHOD,
            #[allow(deprecated)]
            InitializeParams {
                client_info: Some(ClientInfo {
                    name: "LanguageClient-neovim".into(),
                    version: Some((*self.version).clone()),
                }),
                process_id: Some(u64::from(std::process::id())),
                /* deprecated in lsp types, but can't initialize without it */
                root_path: Some(root.clone()),
                root_uri: Some(root.to_url()?),
                initialization_options,
                capabilities: ClientCapabilities {
                    text_document: Some(TextDocumentClientCapabilities {
                        completion: Some(CompletionCapability {
                            completion_item: Some(CompletionItemCapability {
                                snippet_support: Some(has_snippet_support),
                                documentation_format: preferred_markup_kind.clone(),
                                ..CompletionItemCapability::default()
                            }),
                            ..CompletionCapability::default()
                        }),
                        signature_help: Some(SignatureHelpCapability {
                            signature_information: Some(SignatureInformationSettings {
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
                        publish_diagnostics: Some(PublishDiagnosticsCapability {
                            related_information: Some(true),
                            ..PublishDiagnosticsCapability::default()
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
                trace,
                workspace_folders: None,
            },
        )?;

        self.update(|state| {
            state
                .capabilities
                .insert(languageId.clone(), result.clone());
            Ok(())
        })?;

        info!("End {}", lsp::request::Initialize::METHOD);

        if let Err(e) = self.registerCMSource(&languageId, &result) {
            let message = format!("LanguageClient: failed to register as NCM source: {}", e);
            error!("{}\n{:?}", message, e);
            self.vim()?.echoerr(&message)?;
        }
        if let Err(e) = self.registerNCM2Source(&languageId, &result) {
            let message = format!("LanguageClient: failed to register as NCM source: {}", e);
            error!("{}\n{:?}", message, e);
            self.vim()?.echoerr(&message)?;
        }
        if let Err(e) = self.parseSemanticScopes(&languageId, &result) {
            let message = format!("LanguageClient: failed to parse semantic scopes: {}", e);
            error!("{}\n{:?}", message, e);
            self.vim()?.echoerr(&message)?;
        }

        Ok(result)
    }

    fn initialized(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::Initialized::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        self.updateSemanticHighlightTables(&languageId)?;
        self.get_client(&Some(languageId))?
            .notify(lsp::notification::Initialized::METHOD, InitializedParams {})?;
        info!("End {}", lsp::notification::Initialized::METHOD);
        Ok(())
    }

    pub fn textDocument_hover(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::HoverRequest::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let position = self.vim()?.get_position(params)?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::HoverRequest::METHOD,
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

        let hover: Option<Hover> = serde_json::from_value(result.clone())?;
        if let Some(hover) = hover {
            let hoverPreview = self.get(|state| state.hoverPreview)?;
            let use_preview = match hoverPreview {
                HoverPreviewOption::Always => true,
                HoverPreviewOption::Never => false,
                HoverPreviewOption::Auto => hover.lines_len() > 1,
            };
            if use_preview {
                self.preview(&hover)?
            } else {
                self.vim()?.echo_ellipsis(hover.to_string())?
            }
        }

        info!("End {}", lsp::request::HoverRequest::METHOD);
        Ok(result)
    }

    /// Generic find locations, e.g, definitions, references.
    pub fn find_locations(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        let method: String =
            try_get("method", params)?.ok_or_else(|| err_msg("method not found in request!"))?;
        info!("Begin {}", method);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
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

        let result = self.get_client(&Some(languageId))?.call(&method, &params)?;

        if !self.vim()?.get_handle(&params)? {
            return Ok(result);
        }

        let response: Option<GotoDefinitionResponse> = result.clone().to_lsp()?;

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
                let loc = locations.get(0).ok_or_else(|| err_msg("Not found!"))?;
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
                self.display_locations(&locations, &title)?
            }
        }

        info!("End {}", method);
        Ok(result)
    }

    pub fn textDocument_rename(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::Rename::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
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

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::Rename::METHOD,
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

        let edit: WorkspaceEdit = serde_json::from_value(result.clone())?;
        self.apply_WorkspaceEdit(&edit)?;

        info!("End {}", lsp::request::Rename::METHOD);
        Ok(result)
    }

    pub fn textDocument_documentSymbol(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::DocumentSymbolRequest::METHOD);

        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::DocumentSymbolRequest::METHOD,
            DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let syms: <lsp::request::DocumentSymbolRequest as lsp::request::Request>::Result =
            serde_json::from_value(result.clone())?;

        let title = format!("[LC]: symbols for {}", filename);

        let selectionUI = self.get(|state| state.selectionUI)?;
        let selectionUI_autoOpen = self.get(|state| state.selectionUI_autoOpen)?;
        match selectionUI {
            SelectionUI::FZF => {
                let symbols = match syms {
                    Some(lsp::DocumentSymbolResponse::Flat(flat)) => flat
                        .iter()
                        .map(|sym| {
                            let start = sym.location.range.start;
                            format!(
                                "{}:{}:\t{}\t\t{:?}",
                                start.line + 1,
                                start.character + 1,
                                sym.name,
                                sym.kind
                            )
                        })
                        .collect(),
                    Some(lsp::DocumentSymbolResponse::Nested(nested)) => {
                        let mut symbols = Vec::new();

                        fn walk_document_symbol(
                            buffer: &mut Vec<String>,
                            parent: Option<&str>,
                            ds: &lsp::DocumentSymbol,
                        ) {
                            let start = ds.selection_range.start;

                            let name = if let Some(parent) = parent {
                                format!("{}::{}", parent, ds.name)
                            } else {
                                ds.name.clone()
                            };

                            let n = format!(
                                "{}:{}:\t{}\t\t{:?}",
                                start.line + 1,
                                start.character + 1,
                                name,
                                ds.kind
                            );

                            buffer.push(n);

                            if let Some(children) = &ds.children {
                                for child in children {
                                    walk_document_symbol(buffer, Some(&ds.name), child);
                                }
                            }
                        }

                        for ds in &nested {
                            walk_document_symbol(&mut symbols, None, ds);
                        }

                        symbols
                    }
                    _ => Vec::new(),
                };

                self.vim()?.rpcclient.notify(
                    "s:FZF",
                    json!([symbols, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list = match syms {
                    Some(lsp::DocumentSymbolResponse::Flat(flat)) => {
                        flat.iter().map(QuickfixEntry::from_lsp).collect()
                    }
                    Some(lsp::DocumentSymbolResponse::Nested(nested)) => {
                        <Vec<QuickfixEntry>>::from_lsp(&nested)
                    }
                    _ => Ok(Vec::new()),
                };

                let list = list?;
                self.vim()?.setqflist(&list, " ", &title)?;
                if selectionUI_autoOpen {
                    self.vim()?.command("botright copen")?;
                }
                self.vim()?
                    .echo("Document symbols populated to quickfix list.")?;
            }
            SelectionUI::LocationList => {
                let list = match syms {
                    Some(lsp::DocumentSymbolResponse::Flat(flat)) => {
                        flat.iter().map(QuickfixEntry::from_lsp).collect()
                    }
                    Some(lsp::DocumentSymbolResponse::Nested(nested)) => {
                        <Vec<QuickfixEntry>>::from_lsp(&nested)
                    }
                    _ => Ok(Vec::new()),
                };

                let list = list?;
                self.vim()?.setloclist(&list, " ", &title)?;
                if selectionUI_autoOpen {
                    self.vim()?.command("lopen")?;
                }
                self.vim()?
                    .echo("Document symbols populated to location list.")?;
            }
        }

        info!("End {}", lsp::request::DocumentSymbolRequest::METHOD);
        Ok(result)
    }

    pub fn textDocument_codeAction(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::CodeActionRequest::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let position = self.vim()?.get_position(params)?;

        // Unify filename.
        let filename = filename.canonicalize();

        let diagnostics: Vec<_> = self.get(|state| {
            state
                .diagnostics
                .get(&filename)
                .unwrap_or(&vec![])
                .iter()
                .filter(|dn| position >= dn.range.start && position < dn.range.end)
                .cloned()
                .collect()
        })?;

        let result: Value = self.get_client(&Some(languageId))?.call(
            lsp::request::CodeActionRequest::METHOD,
            CodeActionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                range: Range {
                    start: position,
                    end: position,
                },
                context: CodeActionContext {
                    diagnostics,
                    only: None,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )?;

        let response: Option<CodeActionResponse> = serde_json::from_value(result.clone())?;
        let response = response.unwrap_or_else(|| vec![]);

        // Convert any Commands into CodeActions, so that the remainder of the handling can be
        // shared.
        let actions: Vec<_> = response
            .into_iter()
            .map(|action_or_command| match action_or_command {
                CodeActionOrCommand::Command(command) => CodeAction {
                    title: command.title.clone(),
                    kind: Some(command.command.clone()),
                    diagnostics: None,
                    edit: None,
                    command: Some(command),
                    ..CodeAction::default()
                },
                CodeActionOrCommand::CodeAction(action) => action,
            })
            .collect();

        let source: Vec<_> = actions
            .iter()
            .map(|action| {
                format!(
                    "{}: {}",
                    action.kind.as_ref().map_or("action", String::as_ref),
                    action.title
                )
            })
            .collect();

        self.update(|state| {
            state.stashed_codeAction_actions = actions;
            Ok(())
        })?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        self.vim()?
            .rpcclient
            .notify("s:FZF", json!([source, NOTIFICATION__FZFSinkCommand]))?;

        info!("End {}", lsp::request::CodeActionRequest::METHOD);
        Ok(result)
    }

    pub fn textDocument_completion(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::Completion::METHOD);

        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let position = self.vim()?.get_position(params)?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::Completion::METHOD,
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

        info!("End {}", lsp::request::Completion::METHOD);
        Ok(result)
    }

    pub fn textDocument_signatureHelp(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::SignatureHelpRequest::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let position = self.vim()?.get_position(params)?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::SignatureHelpRequest::METHOD,
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

        let help: SignatureHelp = serde_json::from_value(result)?;
        if help.signatures.is_empty() {
            return Ok(Value::Null);
        }

        // active_signature may be negative value.
        // So if it is negative value, we convert it into zero.
        let active_signature_index = help.active_signature.unwrap_or(0).max(0) as usize;

        let active_signature = help
            .signatures
            .get(active_signature_index)
            .ok_or_else(|| err_msg("Failed to get active signature"))?;

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
            decode_parameterLabel(&active_parameter.label, &active_signature.label).ok()
        }) {
            let cmd = format!(
                "echo | echon '{}' | echohl WarningMsg | echon '{}' | echohl None | echon '{}'",
                begin, label, end
            );
            self.vim()?.command(&cmd)?;
        } else {
            self.vim()?.echo(&active_signature.label)?;
        }

        info!("End {}", lsp::request::SignatureHelpRequest::METHOD);
        Ok(Value::Null)
    }

    pub fn textDocument_references(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::References::METHOD);

        let include_declaration: bool = try_get("includeDeclaration", params)?.unwrap_or(true);
        // TODO: cleanup.
        let params = json!({
            "method": lsp::request::References::METHOD,
            "context": ReferenceContext {
                include_declaration,
            }
        })
        .combine(params);
        let result = self.find_locations(&params)?;

        info!("End {}", lsp::request::References::METHOD);
        Ok(result)
    }

    pub fn textDocument_formatting(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::Formatting::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let tab_size = self.vim()?.get_tab_size()?;
        let insert_spaces = self.vim()?.get_insert_spaces(&filename)?;
        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::Formatting::METHOD,
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

        let text_edits: Option<Vec<TextEdit>> = serde_json::from_value(result.clone())?;
        let text_edits = text_edits.unwrap_or_default();
        let edit = lsp::WorkspaceEdit {
            changes: Some(hashmap! {filename.to_url()? => text_edits}),
            document_changes: None,
        };
        self.apply_WorkspaceEdit(&edit)?;
        info!("End {}", lsp::request::Formatting::METHOD);
        Ok(result)
    }

    pub fn textDocument_rangeFormatting(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::RangeFormatting::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let start_line = try_get("range_start_line", params)?
            .map_or_else(|| self.vim()?.eval("LSP#range_start_line()"), Ok)?;
        let end_line = try_get("range_end_line", params)?
            .map_or_else(|| self.vim()?.eval("LSP#range_end_line()"), Ok)?;

        let tab_size = self.vim()?.get_tab_size()?;
        let insert_spaces = self.vim()?.get_insert_spaces(&filename)?;
        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::RangeFormatting::METHOD,
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

        let text_edits: Option<Vec<TextEdit>> = serde_json::from_value(result.clone())?;
        let text_edits = text_edits.unwrap_or_default();
        let edit = lsp::WorkspaceEdit {
            changes: Some(hashmap! {filename.to_url()? => text_edits}),
            document_changes: None,
        };
        self.apply_WorkspaceEdit(&edit)?;
        info!("End {}", lsp::request::RangeFormatting::METHOD);
        Ok(result)
    }

    pub fn completionItem_resolve(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::ResolveCompletionItem::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let completion_item: CompletionItem = try_get("completionItem", params)?
            .ok_or_else(|| err_msg("completionItem not found in request!"))?;

        let result = self
            .get_client(&Some(languageId))?
            .call(lsp::request::ResolveCompletionItem::METHOD, completion_item)?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        // TODO: proper integration.
        let msg = format!("completionItem/resolve result not handled: {:?}", result);
        warn!("{}", msg);
        self.vim()?.echowarn(&msg)?;

        info!("End {}", lsp::request::ResolveCompletionItem::METHOD);
        Ok(Value::Null)
    }

    pub fn workspace_symbol(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::WorkspaceSymbol::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let query = try_get("query", params)?.unwrap_or_default();
        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::WorkspaceSymbol::METHOD,
            WorkspaceSymbolParams {
                query,
                partial_result_params: PartialResultParams::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        )?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        let symbols: Vec<SymbolInformation> = serde_json::from_value(result.clone())?;
        let title = "[LC]: workspace symbols";

        let selectionUI = self.get(|state| state.selectionUI)?;
        let selectionUI_autoOpen = self.get(|state| state.selectionUI_autoOpen)?;
        match selectionUI {
            SelectionUI::FZF => {
                let cwd: String = self.vim()?.eval("getcwd()")?;
                let source: Fallible<Vec<_>> = symbols
                    .iter()
                    .map(|sym| {
                        let filename = sym.location.uri.filepath()?;
                        let relpath = diff_paths(&filename, Path::new(&cwd)).unwrap_or(filename);
                        let start = sym.location.range.start;
                        Ok(format!(
                            "{}:{}:{}:\t{}\t\t{:?}",
                            relpath.to_string_lossy(),
                            start.line + 1,
                            start.character + 1,
                            sym.name,
                            sym.kind
                        ))
                    })
                    .collect();
                let source = source?;

                self.vim()?.rpcclient.notify(
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Fallible<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.vim()?.setqflist(&list, " ", title)?;
                if selectionUI_autoOpen {
                    self.vim()?.command("botright copen")?;
                }
                self.vim()?
                    .echo("Workspace symbols populated to quickfix list.")?;
            }
            SelectionUI::LocationList => {
                let list: Fallible<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.vim()?.setloclist(&list, " ", title)?;
                if selectionUI_autoOpen {
                    self.vim()?.command("lopen")?;
                }
                self.vim()?
                    .echo("Workspace symbols populated to location list.")?;
            }
        }

        info!("End {}", lsp::request::WorkspaceSymbol::METHOD);
        Ok(result)
    }

    pub fn workspace_executeCommand(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::ExecuteCommand::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let command: String =
            try_get("command", params)?.ok_or_else(|| err_msg("command not found in request!"))?;
        let arguments: Vec<Value> = try_get("arguments", params)?.unwrap_or_default();

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::ExecuteCommand::METHOD,
            ExecuteCommandParams {
                command,
                arguments,
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        )?;
        info!("End {}", lsp::request::ExecuteCommand::METHOD);
        Ok(result)
    }

    pub fn workspace_applyEdit(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::ApplyWorkspaceEdit::METHOD);

        let params: ApplyWorkspaceEditParams = params.clone().to_lsp()?;
        self.apply_WorkspaceEdit(&params.edit)?;

        info!("End {}", lsp::request::ApplyWorkspaceEdit::METHOD);

        Ok(serde_json::to_value(ApplyWorkspaceEditResponse {
            applied: true,
        })?)
    }

    pub fn workspace_didChangeConfiguration(&self, params: &Value) -> Fallible<()> {
        info!(
            "Begin {}",
            lsp::notification::DidChangeConfiguration::METHOD
        );
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let settings: Value = try_get("settings", params)?.unwrap_or_default();

        self.get_client(&Some(languageId))?.notify(
            lsp::notification::DidChangeConfiguration::METHOD,
            DidChangeConfigurationParams { settings },
        )?;
        info!("End {}", lsp::notification::DidChangeConfiguration::METHOD);
        Ok(())
    }

    pub fn languageClient_handleCodeLensAction(&self, params: &Value) -> Fallible<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let line = self.vim()?.get_position(params)?.line;

        let code_lens: Vec<CodeLens> = self.get(|state| {
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

        self.update(|state| {
            let actions: Fallible<Vec<_>> = code_lens
                .iter()
                .map(|cl| match &cl.command {
                    None => bail!("no command, skipping"),
                    Some(cmd) => Ok(CodeAction {
                        kind: Some(cmd.title.clone()),
                        title: cmd.command.clone(),
                        command: cl.clone().command,
                        diagnostics: None,
                        edit: None,
                        is_preferred: None,
                    }),
                })
                .filter(|c| c.is_ok())
                .collect();
            state.stashed_codeAction_actions = actions?;
            Ok(())
        })?;

        let source: Fallible<Vec<_>> = code_lens
            .iter()
            .map(|cl| match &cl.command {
                None => bail!("no command, skipping"),
                Some(cmd) => Ok(format!("{}: {}", cmd.title, cmd.command)),
            })
            .filter(|c| c.is_ok())
            .collect();

        self.vim()?
            .rpcclient
            .notify("s:FZF", json!([source?, NOTIFICATION__FZFSinkCommand]))?;

        Ok(Value::Null)
    }

    pub fn textDocument_codeLens(&self, params: &Value) -> Fallible<Value> {
        let use_virtual_text = self.get(|state| state.use_virtual_text.clone())?;
        if UseVirtualText::No == use_virtual_text || UseVirtualText::Diagnostics == use_virtual_text
        {
            return Ok(Value::Null);
        }

        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_languageId(&filename, params)?;
        let capabilities = self.get(|state| state.capabilities.clone())?;
        if let Some(initialize_result) = capabilities.get(&language_id) {
            // XXX: the capabilities state field stores the initialize result, not the capabilities
            // themselves, so we need to deserialize to InitializeResult.
            let initialize_result: InitializeResult =
                serde_json::from_value(initialize_result.clone())?;
            let capabilities = initialize_result.capabilities;

            if let Some(code_lens_provider) = capabilities.code_lens_provider {
                info!("Begin {}", lsp::request::CodeLensRequest::METHOD);
                let client = self.get_client(&Some(language_id))?;
                let input = lsp::CodeLensParams {
                    text_document: TextDocumentIdentifier {
                        uri: filename.to_url()?,
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                };

                let results: Value = client.call(lsp::request::CodeLensRequest::METHOD, &input)?;
                let code_lens: Option<Vec<CodeLens>> = serde_json::from_value(results.clone())?;

                if code_lens_provider.resolve_provider.is_some() {
                    let mut resolved_code_lens = vec![];
                    if let Some(code_lens) = code_lens {
                        for item in code_lens {
                            let mut item = item;
                            if let Some(_d) = &item.data {
                                if let Some(cl) =
                                    client.call(lsp::request::CodeLensResolve::METHOD, &item)?
                                {
                                    item = cl;
                                }
                            }
                            resolved_code_lens.push(item);
                        }
                    }

                    self.update(|state| {
                        state
                            .code_lens
                            .insert(filename.to_owned(), resolved_code_lens);
                        Ok(Value::Null)
                    })?;
                } else if let Some(code_lens) = code_lens {
                    self.update(|state| {
                        state.code_lens.insert(filename.to_owned(), code_lens);
                        Ok(Value::Null)
                    })?;
                }

                info!("End {}", lsp::request::CodeLensRequest::METHOD);
                return Ok(results);
            } else {
                info!(
                    "CodeLens not supported. Skipping {}",
                    lsp::request::CodeLensRequest::METHOD
                );
            }
        }

        Ok(Value::Null)
    }

    pub fn textDocument_didOpen(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidOpenTextDocument::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let text = self.vim()?.get_text(&filename)?;

        let text_document = TextDocumentItem {
            uri: filename.to_url()?,
            language_id: languageId.clone(),
            version: 0,
            text: text.join("\n"),
        };

        self.update(|state| {
            Ok(state
                .text_documents
                .insert(filename.clone(), text_document.clone()))
        })?;

        self.get_client(&Some(languageId.clone()))?.notify(
            lsp::notification::DidOpenTextDocument::METHOD,
            DidOpenTextDocumentParams { text_document },
        )?;

        self.vim()?
            .command("setlocal omnifunc=LanguageClient#complete")?;
        let root = self.get(|state| state.roots.get(&languageId).cloned().unwrap_or_default())?;
        self.vim()?.rpcclient.notify(
            "setbufvar",
            json!([filename, "LanguageClient_projectRoot", root]),
        )?;
        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageClientTextDocumentDidOpenPost")?;

        self.textDocument_codeLens(params)?;

        info!("End {}", lsp::notification::DidOpenTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_didChange(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidChangeTextDocument::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        if !self.get(|state| state.text_documents.contains_key(&filename))? {
            info!("Not opened yet. Switching to didOpen.");
            return self.textDocument_didOpen(params);
        }

        let text = self.vim()?.get_text(&filename)?.join("\n");
        let text_state = self.get(|state| {
            state
                .text_documents
                .get(&filename)
                .map(|d| d.text.clone())
                .unwrap_or_default()
        })?;
        if text == text_state {
            info!("Texts equal. Skipping didChange.");
            return Ok(());
        }

        let version = self.update(|state| {
            let document = state.text_documents.get_mut(&filename).ok_or_else(|| {
                format_err!("Failed to get TextDocumentItem! filename: {}", filename)
            })?;

            let version = document.version + 1;
            document.version = version;
            document.text = text.clone();

            if state.change_throttle.is_some() {
                let metadata = state
                    .text_documents_metadata
                    .entry(filename.clone())
                    .or_insert_with(TextDocumentItemMetadata::default);
                metadata.last_change = Instant::now();
            }
            Ok(version)
        })?;

        self.get_client(&Some(languageId))?.notify(
            lsp::notification::DidChangeTextDocument::METHOD,
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

        self.textDocument_codeLens(params)?;

        info!("End {}", lsp::notification::DidChangeTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_didSave(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidSaveTextDocument::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        if !self.get(|state| state.serverCommands.contains_key(&languageId))? {
            return Ok(());
        }

        let uri = filename.to_url()?;

        self.get_client(&Some(languageId))?.notify(
            lsp::notification::DidSaveTextDocument::METHOD,
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
            },
        )?;

        info!("End {}", lsp::notification::DidSaveTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_didClose(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidCloseTextDocument::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        self.get_client(&Some(languageId))?.notify(
            lsp::notification::DidCloseTextDocument::METHOD,
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
            },
        )?;
        info!("End {}", lsp::notification::DidCloseTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_publishDiagnostics(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::PublishDiagnostics::METHOD);
        let params: PublishDiagnosticsParams = params.clone().to_lsp()?;
        if !self.get(|state| state.diagnosticsEnable)? {
            return Ok(());
        }

        let mut filename = params.uri.filepath()?.to_string_lossy().into_owned();
        // Workaround bug: remove first '/' in case of '/C:/blabla'.
        if filename.chars().next() == Some('/') && filename.chars().nth(2) == Some(':') {
            filename.remove(0);
        }
        // Unify name to avoid mismatch due to case insensitivity.
        let filename = filename.canonicalize();

        let diagnostics_max_severity = self.get(|state| state.diagnostics_max_severity)?;
        let mut diagnostics = params
            .diagnostics
            .iter()
            .filter(|&diagnostic| {
                diagnostic.severity.unwrap_or(DiagnosticSeverity::Hint) <= diagnostics_max_severity
            })
            .map(Clone::clone)
            .collect::<Vec<_>>();

        self.update(|state| {
            state
                .diagnostics
                .insert(filename.clone(), diagnostics.clone());
            Ok(())
        })?;
        self.update_quickfixlist()?;

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
        self.languageClient_handleCursorMoved(&Value::Null)?;
        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageClientDiagnosticsChanged")?;

        info!("End {}", lsp::notification::PublishDiagnostics::METHOD);
        Ok(())
    }

    pub fn textDocument_semanticHighlight(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::SemanticHighlighting::METHOD);
        let mut params: SemanticHighlightingParams = params.clone().to_lsp()?;

        // TODO: Do we need to handle the versioning of the file?
        let mut filename = params
            .text_document
            .uri
            .filepath()?
            .to_string_lossy()
            .into_owned();
        // Workaround bug: remove first '/' in case of '/C:/blabla'.
        if filename.chars().next() == Some('/') && filename.chars().nth(2) == Some(':') {
            filename.remove(0);
        }
        // Unify name to avoid mismatch due to case insensitivity.
        let filename = filename.canonicalize();
        let languageId = self.vim()?.get_languageId(&filename, &Value::Null)?;

        let opt_hl_table = self.get(|state| {
            state
                .semantic_scope_to_hl_group_table
                .get(&languageId)
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

            self.update(|state| {
                state.vim.rpcclient.notify(
                    "s:ApplySemanticHighlights",
                    json!([buffer, ns_id, clears, highlights]),
                )?;

                let old_semantic_hl_state = state
                    .semantic_highlights
                    .insert(languageId.clone(), semantic_hl_state);

                let semantic_hl_state = state.semantic_highlights.get_mut(&languageId).unwrap();

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
            self.update(|state| {
                state
                    .semantic_highlights
                    .insert(languageId.clone(), semantic_hl_state);
                Ok(())
            })?;
        }

        info!("End {}", lsp::notification::SemanticHighlighting::METHOD);
        Ok(())
    }

    pub fn window_logMessage(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::LogMessage::METHOD);
        let params: LogMessageParams = params.clone().to_lsp()?;
        let threshold = self.get(|state| state.windowLogMessageLevel.to_int())??;
        if params.typ.to_int()? > threshold {
            return Ok(());
        }

        let msg = format!("[{:?}] {}", params.typ, params.message);
        self.vim()?.echomsg(&msg)?;
        info!("End {}", lsp::notification::LogMessage::METHOD);
        Ok(())
    }

    pub fn window_showMessage(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::ShowMessage::METHOD);
        let params: ShowMessageParams = params.clone().to_lsp()?;
        let msg = format!("[{:?}] {}", params.typ, params.message);
        self.vim()?.echomsg(&msg)?;
        info!("End {}", lsp::notification::ShowMessage::METHOD);
        Ok(())
    }

    pub fn window_showMessageRequest(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::ShowMessageRequest::METHOD);
        let mut v = Value::Null;
        let msg_params: ShowMessageRequestParams = params.clone().to_lsp()?;
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

        info!("End {}", lsp::request::ShowMessageRequest::METHOD);
        Ok(v)
    }

    pub fn client_registerCapability(&self, languageId: &str, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::RegisterCapability::METHOD);
        let params: RegistrationParams = params.clone().to_lsp()?;
        for r in &params.registrations {
            match r.method.as_str() {
                lsp::notification::DidChangeWatchedFiles::METHOD => {
                    let opt: DidChangeWatchedFilesRegistrationOptions =
                        serde_json::from_value(r.register_options.clone().unwrap_or_default())?;
                    if !self.get(|state| state.watchers.contains_key(languageId))? {
                        let (watcher_tx, watcher_rx) = mpsc::channel();
                        // TODO: configurable duration.
                        let watcher = notify::watcher(watcher_tx, Duration::from_secs(2))?;
                        self.update(|state| {
                            state.watchers.insert(languageId.to_owned(), watcher);
                            state.watcher_rxs.insert(languageId.to_owned(), watcher_rx);
                            Ok(())
                        })?;
                    }

                    self.update(|state| {
                        if let Some(ref mut watcher) = state.watchers.get_mut(languageId) {
                            for w in &opt.watchers {
                                let recursive_mode = if w.glob_pattern.ends_with("**") {
                                    notify::RecursiveMode::Recursive
                                } else {
                                    notify::RecursiveMode::NonRecursive
                                };
                                watcher
                                    .watch(w.glob_pattern.trim_end_matches("**"), recursive_mode)?;
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

        self.update(|state| {
            state.registrations.extend(params.registrations);
            Ok(())
        })?;
        info!("End {}", lsp::request::RegisterCapability::METHOD);
        Ok(Value::Null)
    }

    pub fn client_unregisterCapability(&self, languageId: &str, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::UnregisterCapability::METHOD);
        let params: UnregistrationParams = params.clone().to_lsp()?;
        let mut regs_removed = vec![];
        for r in &params.unregisterations {
            if let Some(idx) = self.get(|state| {
                state
                    .registrations
                    .iter()
                    .position(|i| i.id == r.id && i.method == r.method)
            })? {
                regs_removed.push(self.update(|state| Ok(state.registrations.swap_remove(idx)))?);
            }
        }

        for r in &regs_removed {
            match r.method.as_str() {
                lsp::notification::DidChangeWatchedFiles::METHOD => {
                    let opt: DidChangeWatchedFilesRegistrationOptions =
                        serde_json::from_value(r.register_options.clone().unwrap_or_default())?;
                    self.update(|state| {
                        if let Some(ref mut watcher) = state.watchers.get_mut(languageId) {
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

        info!("End {}", lsp::request::UnregisterCapability::METHOD);
        Ok(Value::Null)
    }

    pub fn exit(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::Exit::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let result = self
            .get_client(&Some(languageId.clone()))?
            .notify(lsp::notification::Exit::METHOD, Value::Null);
        if let Err(err) = result {
            error!("Error: {:?}", err);
        }
        if let Err(err) = self.cleanup(&languageId) {
            error!("Error: {:?}", err);
        }
        info!("End {}", lsp::notification::Exit::METHOD);
        Ok(())
    }

    /////// Extensions by this plugin ///////

    pub fn languageClient_getState(&self, _params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__GetState);
        let s = self.get(|state| serde_json::to_string(state))??;
        info!("End {}", REQUEST__GetState);
        Ok(Value::String(s))
    }

    pub fn languageClient_isAlive(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__IsAlive);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let is_alive = self.get(|state| state.clients.contains_key(&Some(languageId.clone())))?;
        info!("End {}", REQUEST__IsAlive);
        Ok(Value::Bool(is_alive))
    }

    pub fn languageClient_registerServerCommands(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__RegisterServerCommands);
        let commands: HashMap<String, Vec<String>> = params.clone().to_lsp()?;
        self.update(|state| {
            state.serverCommands.extend(commands);
            Ok(())
        })?;
        let exp = format!(
            "let g:LanguageClient_serverCommands={}",
            serde_json::to_string(&self.get(|state| state.serverCommands.clone())?)?
        );
        self.vim()?.command(&exp)?;
        info!("End {}", REQUEST__RegisterServerCommands);
        Ok(Value::Null)
    }

    pub fn languageClient_setLoggingLevel(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__SetLoggingLevel);
        let loggingLevel =
            try_get("loggingLevel", params)?.ok_or_else(|| err_msg("loggingLevel not found!"))?;
        self.update(|state| {
            logger::update_settings(&state.logger, &state.loggingFile, loggingLevel)?;
            state.loggingLevel = loggingLevel;
            Ok(())
        })?;
        info!("End {}", REQUEST__SetLoggingLevel);
        Ok(Value::Null)
    }

    pub fn languageClient_setDiagnosticsList(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__SetDiagnosticsList);
        let diagnosticsList = try_get("diagnosticsList", params)?
            .ok_or_else(|| err_msg("diagnosticsList not found!"))?;
        self.update(|state| {
            state.diagnosticsList = diagnosticsList;
            Ok(())
        })?;
        info!("End {}", REQUEST__SetDiagnosticsList);
        Ok(Value::Null)
    }

    pub fn languageClient_registerHandlers(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__RegisterHandlers);
        let handlers: Fallible<HashMap<String, String>> = params
            .as_object()
            .ok_or_else(|| err_msg("Invalid arguments!"))?
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
        self.update(|state| {
            state.user_handlers.extend(handlers);
            Ok(())
        })?;
        info!("End {}", REQUEST__RegisterHandlers);
        Ok(Value::Null)
    }

    pub fn languageClient_omniComplete(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__OmniComplete);
        let result = self.textDocument_completion(params)?;
        let result: Option<CompletionResponse> = serde_json::from_value(result)?;
        let result = result.unwrap_or_else(|| CompletionResponse::Array(vec![]));
        let matches = match result {
            CompletionResponse::Array(arr) => arr,
            CompletionResponse::List(list) => list.items,
        };

        let complete_position: Option<u64> = try_get("complete_position", params)?;

        let matches: Fallible<Vec<VimCompleteItem>> = matches
            .iter()
            .map(|item| VimCompleteItem::from_lsp(item, complete_position))
            .collect();
        let matches = matches?;
        info!("End {}", REQUEST__OmniComplete);
        Ok(serde_json::to_value(matches)?)
    }

    pub fn languageClient_handleBufNewFile(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__HandleBufNewFile);
        if self.vim()?.get_filename(params)?.is_empty() {
            return Ok(());
        }

        let autoStart: u8 = self
            .vim()?
            .eval("!!get(g:, 'LanguageClient_autoStart', 1)")?;
        if autoStart == 1 {
            let ret = self.languageClient_startServer(params);
            // This is triggered from autocmd, silent all errors.
            if let Err(err) = ret {
                warn!("Failed to start language server automatically. {}", err);
            }
        }

        info!("End {}", NOTIFICATION__HandleBufNewFile);
        Ok(())
    }

    pub fn languageClient_handleFileType(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__HandleFileType);
        if self.vim()?.get_filename(params)?.is_empty() {
            return Ok(());
        }

        let filename = self.vim()?.get_filename(params)?.canonicalize();
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        if self.get(|state| state.clients.contains_key(&Some(languageId.clone())))? {
            self.textDocument_didOpen(params)?;

            if let Some(diagnostics) =
                self.get(|state| state.diagnostics.get(&filename).cloned())?
            {
                self.process_diagnostics(&filename, &diagnostics)?;
                self.languageClient_handleCursorMoved(params)?;
            }
        } else {
            let autoStart: u8 = self
                .vim()?
                .eval("!!get(g:, 'LanguageClient_autoStart', 1)")?;
            if autoStart == 1 {
                let ret = self.languageClient_startServer(params);
                // This is triggered from autocmd, silent all errors.
                if let Err(err) = ret {
                    warn!("Failed to start language server automatically. {}", err);
                }
            }
        }

        info!("End {}", NOTIFICATION__HandleFileType);
        Ok(())
    }

    pub fn languageClient_handleTextChanged(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__HandleTextChanged);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        if !self.get(|state| state.serverCommands.contains_key(&languageId))? {
            return Ok(());
        }

        let skip_notification = self.get(|state| {
            if let Some(metadata) = state.text_documents_metadata.get(&filename) {
                if let Some(throttle) = state.change_throttle {
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

        self.textDocument_didChange(params)?;
        info!("End {}", NOTIFICATION__HandleTextChanged);
        Ok(())
    }

    pub fn languageClient_handleBufWritePost(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__HandleBufWritePost);
        self.textDocument_didSave(params)?;
        info!("End {}", NOTIFICATION__HandleBufWritePost);
        Ok(())
    }

    pub fn languageClient_handleBufDelete(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__HandleBufWritePost);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        if !self.get(|state| state.serverCommands.contains_key(&languageId))? {
            return Ok(());
        }

        self.update(|state| {
            state.text_documents.retain(|f, _| f != &filename);
            state.diagnostics.retain(|f, _| f != &filename);
            state.line_diagnostics.retain(|fl, _| fl.0 != *filename);
            state.signs.retain(|f, _| f != &filename);
            Ok(())
        })?;
        self.textDocument_didClose(params)?;
        info!("End {}", NOTIFICATION__HandleBufWritePost);
        Ok(())
    }

    pub fn languageClient_handleCursorMoved(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__HandleCursorMoved);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let line = self.vim()?.get_position(params)?.line;
        if !self.get(|state| state.serverCommands.contains_key(&languageId))? {
            return Ok(());
        }
        if !self.get(|state| state.diagnostics.contains_key(&filename))?
            && !self.get(|state| state.code_lens.contains_key(&filename))?
        {
            return Ok(());
        }

        if line != self.get(|state| state.last_cursor_line)? {
            let message = self.get(|state| {
                state
                    .line_diagnostics
                    .get(&(filename.clone(), line))
                    .cloned()
                    .unwrap_or_default()
            })?;

            if message != self.get(|state| state.last_line_diagnostic.clone())? {
                self.vim()?.echo_ellipsis(&message)?;
                self.update(|state| {
                    state.last_line_diagnostic = message;
                    Ok(())
                })?;
            }

            self.update(|state| {
                state.last_cursor_line = line;
                Ok(())
            })?;
        }

        let bufnr = self.vim()?.get_bufnr(&filename, params)?;
        let viewport = self.vim()?.get_viewport(params)?;

        // use the most severe diagnostic of each line as the sign
        let signs_next: Vec<_> = self.update(|state| {
            let limit = if let Some(n) = state.diagnosticsSignsMax {
                n as usize
            } else {
                usize::max_value()
            };
            Ok(state
                .diagnostics
                .entry(filename.clone())
                .or_default()
                .iter()
                .filter(|diag| viewport.overlaps(diag.range))
                .map(|diag| {
                    (
                        diag.range.start.line,
                        diag.severity.unwrap_or(DiagnosticSeverity::Hint),
                    )
                })
                .sorted_by_key(|(line, _)| *line)
                .group_by(|(line, _)| *line)
                .into_iter()
                .filter_map(|(_, group)| group.min_by_key(|(_, severity)| *severity))
                .take(limit)
                .map(|(line, severity)| Sign::new(line, format!("LanguageClient{:?}", severity)))
                .collect())
        })?;
        self.update(|state| {
            let signs_prev: Vec<_> = state
                .signs
                .entry(filename.clone())
                .or_default()
                .iter()
                .map(|(_, sign)| sign.clone())
                .collect();
            let mut signs_to_add = vec![];
            let mut signs_to_delete = vec![];
            let diffs = diff::slice(&signs_next, &signs_prev);
            for diff in diffs {
                match diff {
                    diff::Result::Left(s) => {
                        signs_to_add.push(s.clone());
                    }
                    diff::Result::Right(s) => {
                        signs_to_delete.push(s.clone());
                    }
                    _ => {}
                }
            }
            for sign in &mut signs_to_add {
                if sign.id == 0 {
                    state.sign_next_id += 1;
                    sign.id = state.sign_next_id;
                }
            }

            let signs = state.signs.entry(filename.clone()).or_default();
            // signs might be deleted AND added in the same line to change severity,
            // so deletions must be before additions
            for sign in &signs_to_delete {
                signs.remove(&sign.line);
            }
            for sign in &signs_to_add {
                signs.insert(sign.line, sign.clone());
            }
            state
                .vim
                .set_signs(&filename, &signs_to_add, &signs_to_delete)?;
            Ok(())
        })?;

        let highlights: Vec<_> = self.update(|state| {
            Ok(state
                .highlights
                .entry(filename.clone())
                .or_insert_with(|| vec![])
                .iter()
                .filter_map(|h| {
                    if h.line < viewport.start || h.line > viewport.end {
                        return None;
                    }

                    Some(h.clone())
                })
                .collect())
        })?;

        if Some(highlights.clone())
            != self.get(|state| state.highlights_placed.get(&filename).cloned())?
            && self.get(|state| state.is_nvim)?
        {
            let source = if let Some(source) = self.get(|state| state.highlight_source)? {
                source
            } else {
                let source = self
                    .vim()?
                    .rpcclient
                    .call("nvim_buf_add_highlight", json!([0, 0, "Error", 1, 1, 1]))?;
                self.update(|state| {
                    state.highlight_source = Some(source);
                    Ok(())
                })?;
                source
            };

            self.update(|state| {
                state
                    .highlights_placed
                    .insert(filename.clone(), highlights.clone());
                Ok(())
            })?;

            self.vim()?.rpcclient.notify(
                "nvim_buf_clear_highlight",
                json!([0, source, viewport.start, viewport.end]),
            )?;

            self.vim()?
                .rpcclient
                .notify("s:AddHighlights", json!([source, highlights]))?;
        }

        let mut virtual_texts = vec![];
        let use_virtual_text = self.get(|state| state.use_virtual_text.clone())?;

        // diagnostics
        if UseVirtualText::All == use_virtual_text
            || UseVirtualText::Diagnostics == use_virtual_text
        {
            let diagnostics = self.get(|state| state.diagnostics.clone())?;
            let diagnosticsDisplay = self.get(|state| state.diagnosticsDisplay.clone())?;
            let diag_list = diagnostics.get(&filename);
            if let Some(diag_list) = diag_list {
                for diag in diag_list {
                    if viewport.overlaps(diag.range) {
                        virtual_texts.push(VirtualText {
                            line: diag.range.start.line,
                            text: diag.message.replace("\n", "  ").clone(),
                            hl_group: diagnosticsDisplay
                                .get(&(diag.severity.unwrap_or(DiagnosticSeverity::Hint) as u64))
                                .ok_or_else(|| err_msg("Failed to get display"))?
                                .virtualTexthl
                                .clone(),
                        });
                    }
                }
            }
        }

        // code lens
        if UseVirtualText::All == use_virtual_text || UseVirtualText::CodeLens == use_virtual_text {
            let filename = self.vim()?.get_filename(params)?;
            let code_lenses =
                self.get(|state| state.code_lens.get(&filename).cloned().unwrap_or_default())?;

            for cl in code_lenses {
                if let Some(command) = cl.command {
                    let line = cl.range.start.line;
                    let text = command.title;

                    match virtual_texts.iter().position(|v| v.line == line) {
                        Some(idx) => virtual_texts[idx]
                            .text
                            .push_str(format!(" | {}", text).as_str()),
                        None => virtual_texts.push(VirtualText {
                            line,
                            text,
                            hl_group: "Comment".into(),
                        }),
                    }
                }
            }
        }

        if self.get(|state| state.is_nvim)? {
            let namespace_id = self.get_or_create_namespace(&LCNamespace::VirtualText)?;
            self.vim()?.set_virtual_texts(
                bufnr,
                namespace_id,
                viewport.start,
                viewport.end,
                &virtual_texts,
            )?;
        }

        info!("End {}", NOTIFICATION__HandleCursorMoved);
        Ok(())
    }

    pub fn languageClient_handleCompleteDone(&self, params: &Value) -> Fallible<()> {
        let filename = self.vim()?.get_filename(params)?;
        let position = self.vim()?.get_position(params)?;
        let completed_item: VimCompleteItem = try_get("completed_item", params)?
            .ok_or_else(|| err_msg("completed_item not found!"))?;

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
        if self.get(|state| state.completionPreferTextEdit)? {
            if let Some(edit) = lspitem.text_edit {
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
            };
        }

        if self.get(|state| state.applyCompletionAdditionalTextEdits)? {
            if let Some(aedits) = lspitem.additional_text_edits {
                edits.extend(aedits);
            };
        }

        if edits.is_empty() {
            return Ok(());
        }

        let position = self.apply_TextEdits(filename, &edits, position)?;
        self.vim()?
            .cursor(position.line + 1, position.character + 1)
    }

    pub fn languageClient_FZFSinkLocation(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__FZFSinkLocation);
        let params = match params {
            Value::Array(ref arr) => Value::Array(arr.clone()),
            _ => {
                bail!("Expecting array params!");
            }
        };

        let lines: Vec<String> = serde_json::from_value(params.clone())?;
        if lines.is_empty() {
            err_msg("No selection!");
        }

        let location = lines
            .get(0)
            .ok_or_else(|| format_err!("Failed to get line! lines: {:?}", lines))?
            .split('\t')
            .next()
            .ok_or_else(|| format_err!("Failed to parse: {:?}", lines))?;
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
            .ok_or_else(|| format_err!("Failed to get line! tokens: {:?}", tokens))?
            .to_int()?
            - 1;
        let character = tokens_iter
            .next()
            .ok_or_else(|| format_err!("Failed to get character! tokens: {:?}", tokens))?
            .to_int()?
            - 1;

        self.edit(&None, &filename)?;
        self.vim()?.cursor(line + 1, character + 1)?;

        info!("End {}", NOTIFICATION__FZFSinkLocation);
        Ok(())
    }

    pub fn languageClient_FZFSinkCommand(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__FZFSinkCommand);
        let selection: String =
            try_get("selection", params)?.ok_or_else(|| err_msg("selection not found!"))?;
        let tokens: Vec<&str> = selection.splitn(2, ": ").collect();
        let kind = tokens
            .get(0)
            .cloned()
            .ok_or_else(|| format_err!("Failed to get kind! tokens: {:?}", tokens))?;
        let title = tokens
            .get(1)
            .cloned()
            .ok_or_else(|| format_err!("Failed to get title! tokens: {:?}", tokens))?;
        let action = self.get(|state| {
            let actions = &state.stashed_codeAction_actions;

            actions
                .iter()
                .find(|action| {
                    action.kind.as_ref().map_or(kind, String::as_ref) == kind
                        && action.title == title
                })
                .cloned()
                .ok_or_else(|| {
                    format_err!("No stashed action found! stashed actions: {:?}", actions)
                })
        })??;

        // Apply edit before command.
        if let Some(edit) = &action.edit {
            self.apply_WorkspaceEdit(edit)?;
        }

        if let Some(command) = &action.command {
            if !self.try_handle_command_by_client(&command)? {
                let params = json!({
                "command": command.command,
                "arguments": command.arguments,
                });
                self.workspace_executeCommand(&params)?;
            }
        }

        self.update(|state| {
            state.stashed_codeAction_actions = vec![];
            Ok(())
        })?;

        info!("End {}", NOTIFICATION__FZFSinkCommand);
        Ok(())
    }

    pub fn languageClient_semanticScopes(&self, params: &Value) -> Fallible<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let (scopes, mut scope_mapping) = self.get(|state| {
            (
                state
                    .semantic_scopes
                    .get(&languageId)
                    .cloned()
                    .unwrap_or_default(),
                state
                    .semantic_scope_to_hl_group_table
                    .get(&languageId)
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

    pub fn languageClient_semanticHlSyms(&self, params: &Value) -> Fallible<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let (opt_scopes, opt_hl_state) = self.get(|state| {
            (
                state.semantic_scopes.get(&languageId).cloned(),
                state.semantic_highlights.get(&languageId).cloned(),
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

    pub fn NCM_refresh(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__NCMRefresh);
        let params: NCMRefreshParams = serde_json::from_value(rpc::to_value(params.clone())?)?;
        let NCMRefreshParams { info, ctx } = params;
        if ctx.typed.is_empty() {
            return Ok(Value::Null);
        }

        let filename = ctx.filepath.clone();
        let line = ctx.lnum - 1;
        let character = ctx.col - 1;

        let result = self.textDocument_completion(&json!({
            "languageId": ctx.filetype,
            "filename": filename,
            "line": line,
            "character": character,
            "handle": false,
        }))?;
        let result: Option<CompletionResponse> = serde_json::from_value(result)?;
        let result = result.unwrap_or_else(|| CompletionResponse::Array(vec![]));
        let is_incomplete = match result {
            CompletionResponse::Array(_) => false,
            CompletionResponse::List(ref list) => list.is_incomplete,
        };
        let matches: Fallible<Vec<VimCompleteItem>> = match result {
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
        info!("End {}", REQUEST__NCMRefresh);
        Ok(Value::Null)
    }

    pub fn NCM2_on_complete(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__NCM2OnComplete);

        let orig_ctx: Value = serde_json::from_value(rpc::to_value(params.clone())?)?;
        let orig_ctx = &orig_ctx["ctx"];

        let ctx: NCM2Context = serde_json::from_value(orig_ctx.clone())?;
        if ctx.typed.is_empty() {
            return Ok(Value::Null);
        }

        let filename = ctx.filepath.clone();
        let line = ctx.lnum - 1;
        let character = ctx.ccol - 1;

        let result = self.textDocument_completion(&json!({
                "languageId": ctx.filetype,
                "filename": filename,
                "line": line,
                "character": character,
                "handle": false}));
        let is_incomplete;
        let matches;
        if let Ok(ref value) = result {
            let completion = serde_json::from_value(value.clone())?;
            is_incomplete = match completion {
                CompletionResponse::List(ref list) => list.is_incomplete,
                _ => false,
            };
            let matches_result: Fallible<Vec<VimCompleteItem>> = match completion {
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
        info!("End {}", REQUEST__NCM2OnComplete);
        result
    }

    pub fn languageClient_explainErrorAtPoint(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__ExplainErrorAtPoint);
        let filename = self.vim()?.get_filename(params)?;
        let position = self.vim()?.get_position(params)?;
        let diag = self.get(|state| {
            state
                .diagnostics
                .get(&filename)
                .ok_or_else(|| format_err!("No diagnostics found: filename: {}", filename,))?
                .iter()
                .find(|dn| position >= dn.range.start && position < dn.range.end)
                .cloned()
                .ok_or_else(|| {
                    format_err!(
                        "No diagnostics found: filename: {}, line: {}, character: {}",
                        filename,
                        position.line,
                        position.character
                    )
                })
        })??;

        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let root = self.get(|state| state.roots.get(&languageId).cloned().unwrap_or_default())?;
        let rootUri = root.to_url()?;

        let mut explanation = diag.message;
        if let Some(related_information) = diag.related_information {
            explanation = format!("{}\n", explanation);
            for ri in related_information {
                let prefix = format!("{}/", rootUri);
                let uri = if ri.location.uri.as_str().starts_with(prefix.as_str()) {
                    // Heuristic: if start of stringified URI matches rootUri, abbreviate it away
                    &ri.location.uri.as_str()[rootUri.as_str().len() + 1..]
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

        self.preview(explanation.as_str())?;

        info!("End {}", REQUEST__ExplainErrorAtPoint);
        Ok(Value::Null)
    }

    // Extensions by language servers.
    pub fn language_status(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__LanguageStatus);
        let params: LanguageStatusParams = params.clone().to_lsp()?;
        let msg = format!("{} {}", params.typee, params.message);
        self.vim()?.echomsg(&msg)?;
        info!("End {}", NOTIFICATION__LanguageStatus);
        Ok(())
    }

    pub fn rust_handleBeginBuild(&self, _params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__RustBeginBuild);
        self.vim()?.command(vec![
            format!("let {}=1", VIM__ServerStatus),
            format!("let {}='Rust: build begin'", VIM__ServerStatusMessage),
        ])?;
        info!("End {}", NOTIFICATION__RustBeginBuild);
        Ok(())
    }

    pub fn rust_handleDiagnosticsBegin(&self, _params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__RustDiagnosticsBegin);
        self.vim()?.command(vec![
            format!("let {}=1", VIM__ServerStatus),
            format!("let {}='Rust: diagnostics begin'", VIM__ServerStatusMessage),
        ])?;
        info!("End {}", NOTIFICATION__RustDiagnosticsBegin);
        Ok(())
    }

    pub fn rust_handleDiagnosticsEnd(&self, _params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__RustDiagnosticsEnd);
        self.vim()?.command(vec![
            format!("let {}=0", VIM__ServerStatus),
            format!("let {}='Rust: diagnostics end'", VIM__ServerStatusMessage),
        ])?;
        info!("End {}", NOTIFICATION__RustDiagnosticsEnd);
        Ok(())
    }

    pub fn window_progress(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__WindowProgress);
        let params: WindowProgressParams = params.clone().to_lsp()?;

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
            format!("let {}={}", VIM__ServerStatus, if done { 0 } else { 1 }),
            format!(
                "let {}='{}'",
                VIM__ServerStatusMessage,
                &escape_single_quote(buf)
            ),
        ])?;
        info!("End {}", NOTIFICATION__WindowProgress);
        Ok(())
    }

    pub fn languageClient_startServer(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__StartServer);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
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
        let mutex_for_language_id = self.get_client_update_mutex(Some(languageId.clone()))?;
        let _raii_lock: MutexGuard<()> = mutex_for_language_id.lock().map_err(|err| {
            format_err!(
                "Failed to lock client creation for languageId {:?}: {:?}",
                languageId,
                err
            )
        })?;

        if self.get(|state| state.clients.contains_key(&Some(languageId.clone())))? {
            return Ok(json!({}));
        }

        self.sync_settings()?;
        info!("settings synced");

        let command = self
            .get(|state| state.serverCommands.get(&languageId).cloned())?
            .ok_or_else(|| {
                Error::from(LCError::NoServerCommands {
                    languageId: languageId.clone(),
                })
            })?;

        let rootPath: Option<String> = try_get("rootPath", &params)?;
        let root = if let Some(r) = rootPath {
            r
        } else {
            get_rootPath(
                Path::new(&filename),
                &languageId,
                &self.get(|state| state.rootMarkers.clone())?,
            )?
            .to_string_lossy()
            .into()
        };
        let message = format!("Project root: {}", root);
        if self.get(|state| state.echo_project_root)? {
            self.vim()?.echomsg_ellipsis(&message)?;
        }
        info!("{}", message);
        self.update(|state| {
            state.roots.insert(languageId.clone(), root.clone());
            Ok(())
        })?;

        let (child_id, reader, writer): (_, Box<dyn SyncRead>, Box<dyn SyncWrite>) =
            if command.get(0).map(|c| c.starts_with("tcp://")) == Some(true) {
                let addr = command
                    .get(0)
                    .map(|s| s.replace("tcp://", ""))
                    .ok_or_else(|| err_msg("Server command can't be empty!"))?;
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

                let stderr = match self.get(|state| state.serverStderr.clone())? {
                    Some(ref path) => std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .with_context(|err| format!("Failed to open file ({}): {}", path, err))?
                        .into(),
                    None => Stdio::null(),
                };

                let process = std::process::Command::new(
                    command.get(0).ok_or_else(|| err_msg("Empty command!"))?,
                )
                .args(&command[1..])
                .current_dir(&root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(stderr)
                .spawn()
                .with_context(|err| {
                    format!("Failed to start language server ({:?}): {}", command, err)
                })?;

                let child_id = Some(process.id());
                let reader = Box::new(BufReader::new(
                    process
                        .stdout
                        .ok_or_else(|| err_msg("Failed to get subprocess stdout"))?,
                ));
                let writer = Box::new(BufWriter::new(
                    process
                        .stdin
                        .ok_or_else(|| err_msg("Failed to get subprocess stdin"))?,
                ));
                (child_id, reader, writer)
            };

        let client = RpcClient::new(
            Some(languageId.clone()),
            reader,
            writer,
            child_id,
            self.get(|state| state.tx.clone())?,
        )?;
        self.update(|state| {
            state.clients.insert(Some(languageId.clone()), client);
            Ok(())
        })?;

        info!("End {}", REQUEST__StartServer);

        if self.get(|state| state.clients.len())? == 2 {
            self.define_signs()?;
        }

        self.initialize(&params)?;
        self.initialized(&params)?;

        let root = self.get(|state| state.roots.get(&languageId).cloned().unwrap_or_default())?;
        match self.get_workspace_settings(&root) {
            Ok(Value::Null) => (),
            Ok(settings) => self.workspace_didChangeConfiguration(&json!({
                "languageId": languageId,
                "settings": settings,
            }))?,
            Err(err) => warn!("Failed to get workspace settings: {}", err),
        }

        self.textDocument_didOpen(&params)?;
        self.textDocument_didChange(&params)?;

        self.vim()?
            .rpcclient
            .notify("s:ExecuteAutocmd", "LanguageClientStarted")?;
        Ok(Value::Null)
    }

    pub fn languageClient_serverExited(&self, params: &Value) -> Fallible<()> {
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let message: String = try_get("message", params)?.unwrap_or_default();

        if self.get(|state| state.clients.contains_key(&Some(languageId.clone())))? {
            if let Err(err) = self.cleanup(&languageId) {
                error!("Error in cleanup: {:?}", err);
            }
            if let Err(err) = self.vim()?.echoerr(format!(
                "Language server {} exited unexpectedly: {}",
                languageId, message
            )) {
                error!("Error in echoerr: {:?}", err);
            }
        }

        Ok(())
    }

    pub fn handle_fs_events(&self) -> Fallible<()> {
        let mut pending_changes = HashMap::new();
        self.update(|state| {
            for (languageId, watcher_rx) in &mut state.watcher_rxs {
                let mut events = vec![];
                loop {
                    let result = watcher_rx.try_recv();
                    let event = match result {
                        Ok(event) => event,
                        Err(mpsc::TryRecvError::Empty) => {
                            break;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            bail!("File system notification channel disconnected!");
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

                pending_changes.insert(languageId.to_owned(), changes);
            }
            Ok(())
        })?;

        for (languageId, changes) in pending_changes {
            self.workspace_didChangeWatchedFiles(&json!({
                "languageId": languageId,
                "changes": changes
            }))?;
        }

        Ok(())
    }

    pub fn workspace_didChangeWatchedFiles(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidChangeWatchedFiles::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let params: DidChangeWatchedFilesParams = params.clone().to_lsp()?;
        self.get_client(&Some(languageId))?
            .notify(lsp::notification::DidChangeWatchedFiles::METHOD, params)?;

        info!("End {}", lsp::notification::DidChangeWatchedFiles::METHOD);
        Ok(())
    }

    pub fn java_classFileContents(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__ClassFileContents);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let content: String = self
            .get_client(&Some(languageId))?
            .call(REQUEST__ClassFileContents, params)?;

        let lines: Vec<String> = content
            .lines()
            .map(std::string::ToString::to_string)
            .collect();

        let goto_cmd = self
            .vim()?
            .get_goto_cmd(params)?
            .unwrap_or_else(|| "edit".to_string());

        let uri: String =
            try_get("uri", params)?.ok_or_else(|| err_msg("uri not found in request!"))?;

        self.vim()?
            .rpcclient
            .notify("s:Edit", json!([goto_cmd, uri]))?;

        self.vim()?.setline(1, &lines)?;
        self.vim()?
            .command("setlocal buftype=nofile filetype=java noswapfile")?;

        info!("End {}", REQUEST__ClassFileContents);
        Ok(Value::String(content))
    }

    pub fn debug_info(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__DebugInfo);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        let mut msg = String::new();
        self.get(|state| {
            msg += &format!(
                "Project root: {}\n",
                state.roots.get(&languageId).cloned().unwrap_or_default()
            );
            msg += &format!(
                "Language server process id: {:?}\n",
                state
                    .clients
                    .get(&Some(languageId.clone()))
                    .map(|c| c.process_id)
                    .unwrap_or_default(),
            );
            msg += &format!(
                "Language server stderr: {}\n",
                state.serverStderr.clone().unwrap_or_default()
            );
            msg += &format!("Log level: {}\n", state.loggingLevel);
            msg += &format!(
                "Log file: {}\n",
                state.loggingFile.clone().unwrap_or_default()
            );
        })?;
        self.vim()?.echo(&msg)?;
        info!("End {}", REQUEST__DebugInfo);
        Ok(json!(msg))
    }
}
