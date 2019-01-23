use super::*;
use crate::vim::VirtualText;

use crate::language_client::LanguageClient;
use crate::lsp::notification::Notification;
use crate::lsp::request::GotoDefinitionResponse;
use crate::lsp::request::Request;
use crate::rpcclient::RpcClient;
use notify::Watcher;
use std::sync::mpsc;

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

    pub fn loop_call(&self, rx: &crossbeam_channel::Receiver<Call>) -> Fallible<()> {
        for call in rx.iter() {
            let language_client = LanguageClient(self.0.clone());
            thread::spawn(move || {
                if let Err(err) = language_client.handle_call(call) {
                    error!("Error handling request:\n{:?}", err);
                }
            });
        }

        Ok(())
    }

    /////// Utils ///////

    pub fn gather_args<E: VimExp + std::fmt::Debug, T: DeserializeOwned>(
        &self,
        exps: &[E],
        map: &Value,
    ) -> Fallible<T> {
        let mut map = map
            .as_object()
            .cloned()
            .unwrap_or_else(serde_json::Map::new);
        let mut keys_request = vec![];
        let mut exps_request = vec![];
        for e in exps {
            let k = e.to_key();
            if !map.contains_key(&k) {
                keys_request.push(k);
                exps_request.push(e.to_exp());
            }
        }
        let values_request: Vec<Value> = if keys_request.is_empty() {
            vec![]
        } else {
            info!(
                "Some arguments are not available. Requesting from vim. Keys: {:?}. Exps: {:?}",
                keys_request, exps_request,
            );
            self.eval::<&[_], _>(exps_request.as_ref())?
        };
        for (k, v) in keys_request.into_iter().zip(values_request.into_iter()) {
            map.insert(k, v);
        }

        let mut result = vec![];
        for e in exps {
            let k = e.to_key();
            result.push(
                map.remove(&k)
                    .ok_or_else(|| format_err!("Failed to get value! k: {}", k))?,
            );
        }

        info!("gather_args: {:?} = {:?}", exps, result);
        Ok(serde_json::from_value(Value::Array(result))?)
    }

    fn sync_settings(&self) -> Fallible<()> {
        info!("Begin sync settings");
        let (loggingFile, loggingLevel, serverStderr): (
            Option<String>,
            log::LevelFilter,
            Option<String>,
        ) = self.eval(
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
        ) = self.eval(
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

        let (diagnosticsSignsMax, documentHighlightDisplay, selectionUI_autoOpen, use_virtual_text): (
            Option<u64>,
            Value,
            u8,
            u8,
        ) = self.eval(
            [
                "get(g:, 'LanguageClient_diagnosticsSignsMax', v:null)",
                "get(g:, 'LanguageClient_documentHighlightDisplay', {})",
                "!!s:GetVar('LanguageClient_selectionUI_autoOpen', 1)",
                "s:useVirtualText()",
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
        } else if self.eval::<_, i64>("get(g:, 'loaded_fzf')")? == 1 {
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

        let is_nvim = is_nvim == 1;

        self.update(|state| {
            state.autoStart = autoStart;
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
            state.use_virtual_text = use_virtual_text == 1;
            state.loggingFile = loggingFile;
            state.loggingLevel = loggingLevel;
            state.serverStderr = serverStderr;
            state.is_nvim = is_nvim;
            Ok(())
        })?;

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

        self.command(cmds)?;
        Ok(())
    }

    fn apply_WorkspaceEdit(&self, edit: &WorkspaceEdit, params: &Value) -> Fallible<()> {
        debug!(
            "Begin apply WorkspaceEdit: {:?}. Params: {:?}",
            edit, params
        );
        let (filename, line, character): (String, u64, u64) =
            self.gather_args(&[VimVar::Filename, VimVar::Line, VimVar::Character], params)?;

        if let Some(ref changes) = edit.document_changes {
            match changes {
                DocumentChanges::Edits(ref changes) => {
                    for e in changes {
                        self.apply_TextEdits(&e.text_document.uri.filepath()?, &e.edits)?;
                    }
                }
                DocumentChanges::Operations(ref ops) => {
                    for op in ops {
                        if let DocumentChangeOperation::Edit(ref e) = op {
                            self.apply_TextEdits(&e.text_document.uri.filepath()?, &e.edits)?;
                        }
                        // TODO: handle ResourceOp.
                    }
                }
            }
        }
        if let Some(ref changes) = edit.changes {
            for (uri, edits) in changes {
                self.apply_TextEdits(&uri.filepath()?, edits)?;
            }
        }
        self.edit(&None, &filename)?;
        self.cursor(line + 1, character + 1)?;
        debug!("End apply WorkspaceEdit");
        Ok(())
    }

    pub fn textDocument_documentHighlight(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::DocumentHighlightRequest::METHOD);
        let (languageId, filename, line, character, handle): (String, String, u64, u64, bool) =
            self.gather_args(
                &[
                    VimVar::LanguageId,
                    VimVar::Filename,
                    VimVar::Line,
                    VimVar::Character,
                    VimVar::Handle,
                ],
                params,
            )?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::DocumentHighlightRequest::METHOD,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position: Position { line, character },
            },
        )?;

        if !handle {
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

            let buffer = self.vim()?.call("nvim_win_get_buf", json!([0]))?;

            let source = if let Some(hs) = self.get(|state| state.document_highlight_source)? {
                if hs.buffer == buffer {
                    // If we want to highlight in the same buffer as last time, we can reuse
                    // the previous source.
                    Some(hs.source)
                } else {
                    // Clear the highlight in the previous buffer.
                    self.vim()?.notify(
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
                    let source = self.vim()?.call(
                        "nvim_buf_add_highlight",
                        json!([buffer, 0, "Error", 1, 1, 1]),
                    )?;
                    self.update(|state| {
                        state.document_highlight_source = Some(HighlightSource { buffer, source });
                        Ok(())
                    })?;
                    source
                }
            };

            self.vim()?
                .notify("nvim_buf_clear_highlight", json!([buffer, source, 0, -1]))?;
            self.vim()?
                .notify("s:AddHighlights", json!([source, highlights]))?;
        }

        info!("End {}", lsp::request::DocumentHighlightRequest::METHOD);
        Ok(result)
    }

    pub fn languageClient_clearDocumentHighlight(&self, _: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__ClearDocumentHighlight);

        let buffer_source = self.update(|state| Ok(state.document_highlight_source.take()))?;
        if let Some(HighlightSource { buffer, source }) = buffer_source {
            self.vim()?
                .notify("nvim_buf_clear_highlight", json!([buffer, source, 0, -1]))?;
        }

        info!("End {}", NOTIFICATION__ClearDocumentHighlight);
        Ok(())
    }

    fn apply_TextEdits<P: AsRef<Path>>(&self, path: P, edits: &[TextEdit]) -> Fallible<()> {
        debug!("Begin apply TextEdits: {:?}", edits);
        if edits.is_empty() {
            return Ok(());
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

        let mut lines: Vec<String> = self.vim()?.call("getline", json!([1, '$']))?;
        let lines_len_prev = lines.len();
        let fixendofline = self.eval::<_, u8>("&fixendofline")? == 1;
        if lines.last().map(String::is_empty) == Some(false) && fixendofline {
            lines.push("".to_owned());
        }

        let mut lines = apply_TextEdits(&lines, &edits)?;

        if lines.last().map(String::is_empty) == Some(true) && fixendofline {
            lines.pop();
        }
        if lines.len() < lines_len_prev {
            self.command(format!("{},{}d", lines.len() + 1, lines_len_prev))?;
        }
        self.vim()?.notify("setline", json!([1, lines]))?;
        debug!("End apply TextEdits");
        Ok(())
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
                self.setqflist(&qflist, "r", title)?;
            }
            DiagnosticsList::Location => {
                self.setloclist(&qflist, "r", title)?;
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
                msg += &format!("[{:?}]", severity);
            }
            if let Some(ref code) = entry.code {
                let s = code.to_string();
                if !s.is_empty() {
                    msg += &format!("[{}]", s);
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

        // Signs.
        let mut signs: Vec<_> = diagnostics
            .iter()
            .map(|dn| {
                let line = dn.range.start.line;
                let text = lines
                    .get(line as usize)
                    .map(|l| l.to_string())
                    .unwrap_or_default();

                Sign::new(line + 1, text, dn.severity)
            })
            .collect();

        // There might be multiple diagnostics for one line. Show only highest severity.
        signs.sort_unstable();
        signs.dedup();
        if let Some(diagnosticSignsMax) = self.get(|state| state.diagnosticsSignsMax)? {
            signs.truncate(diagnosticSignsMax as usize);
        }

        self.update(|state| {
            state.signs.insert(filename.to_owned(), signs);
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
            self.vim()?.notify("s:MatchDelete", json!([ids]))?;

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
                            middleLines.push(endLine);
                            middleLines
                        }
                    })
                    .collect();

                let match_id = self.vim()?.call("matchaddpos", json!([hl_group, ranges]))?;
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
                let cwd: String = self.eval("getcwd()")?;
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

                self.vim()?.notify(
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
                self.setqflist(&list, " ", title)?;
                if selectionUI_autoOpen {
                    self.command("botright copen")?;
                }
                self.echo("Quickfix list updated.")?;
            }
            SelectionUI::LocationList => {
                let list: Fallible<Vec<_>> = locations
                    .iter()
                    .map(|loc| location_to_quickfix_entry(self, loc))
                    .collect();
                let list = list?;
                self.setloclist(&list, " ", title)?;
                if selectionUI_autoOpen {
                    self.command("lopen")?;
                }
                self.echo("Location list updated.")?;
            }
        }
        Ok(())
    }

    fn registerCMSource(&self, languageId: &str, result: &Value) -> Fallible<()> {
        info!("Begin register NCM source");
        let exists_CMRegister: u64 = self.eval("exists('g:cm_matcher')")?;
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

        self.vim()?.notify(
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
        let exists_ncm2: u64 = self.eval("exists('g:ncm2_loaded')")?;
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

        self.vim()?.notify(
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

    fn get_line(&self, path: impl AsRef<Path>, line: u64) -> Fallible<String> {
        let value = self.vim()?.call(
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
        if !CommandsClient.contains(&cmd.command.as_str()) {
            return Ok(false);
        }

        if cmd.command == "java.apply.workspaceEdit" {
            if let Some(ref edits) = cmd.arguments {
                for edit in edits {
                    let edit: WorkspaceEdit = serde_json::from_value(edit.clone())?;
                    self.apply_WorkspaceEdit(&edit, &Value::Null)?;
                }
            }
        } else {
            bail!("Not implemented: {}", cmd.command);
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
        self.get(|state| {
            for f in state.diagnostics.keys() {
                if f.starts_with(&root) {
                    filenames.push(f.clone());
                }
            }
        })?;
        for f in filenames {
            self.process_diagnostics(&f, &[])?;
        }
        self.languageClient_handleCursorMoved(&Value::Null)?;

        self.update(|state| {
            state.diagnostics.retain(|f, _| !f.starts_with(&root));
            state.clients.remove(&Some(languageId.into()));
            state.last_cursor_line = 0;
            state.text_documents.retain(|f, _| !f.starts_with(&root));
            state.roots.remove(languageId);
            Ok(())
        })?;
        self.update_quickfixlist()?;

        self.command(vec![
            format!("let {}=0", VIM__ServerStatus),
            format!("let {}=''", VIM__ServerStatusMessage),
        ])?;
        self.vim()?
            .notify("s:ExecuteAutocmd", "LanguageClientStopped")?;

        info!("End cleanup");
        Ok(())
    }

    fn preview<D>(&self, to_display: &D) -> Fallible<()>
    where
        D: ToDisplay + ?Sized,
    {
        let bufname = "__LanguageClient__";

        let cmd = "silent! pedit! +setlocal\\ buftype=nofile\\ nobuflisted\\ noswapfile\\ nonumber";
        let cmd = if let Some(ref ft) = to_display.vim_filetype() {
            format!("{}\\ filetype={} {}", cmd, ft, bufname)
        } else {
            format!("{} {}", cmd, bufname)
        };
        self.command(cmd)?;

        let lines = to_display.to_display();
        if self.get(|state| state.is_nvim)? {
            let bufnr: u64 = serde_json::from_value(self.vim()?.call("bufnr", bufname)?)?;
            self.vim()?
                .notify("nvim_buf_set_lines", json!([bufnr, 0, -1, 0, lines]))?;
        } else {
            self.vim()?
                .notify("setbufline", json!([bufname, 1, lines]))?;
            // TODO: removing existing bottom lines.
        }

        Ok(())
    }

    /////// LSP ///////

    fn initialize(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::Initialize::METHOD);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let (has_snippet_support,): (u64,) =
            self.gather_args(&[("hasSnippetSupport", "s:hasSnippetSupport()")], params)?;
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

        let trace = self.get(|state| state.trace.clone())?;

        let result: Value = self.get_client(&Some(languageId.clone()))?.call(
            lsp::request::Initialize::METHOD,
            InitializeParams {
                process_id: Some(u64::from(std::process::id())),
                root_path: Some(root.clone()),
                root_uri: Some(root.to_url()?),
                initialization_options,
                capabilities: ClientCapabilities {
                    text_document: Some(TextDocumentClientCapabilities {
                        completion: Some(CompletionCapability {
                            completion_item: Some(CompletionItemCapability {
                                snippet_support: Some(has_snippet_support),
                                ..CompletionItemCapability::default()
                            }),
                            ..CompletionCapability::default()
                        }),
                        signature_help: Some(SignatureHelpCapability {
                            signature_information: Some(SignatureInformationSettings {
                                documentation_format: None,
                                parameter_information: Some(ParameterInformationSettings {
                                    label_offset_support: Some(true),
                                }),
                            }),
                            ..SignatureHelpCapability::default()
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
            self.echoerr(&message)?;
        }
        if let Err(e) = self.registerNCM2Source(&languageId, &result) {
            let message = format!("LanguageClient: failed to register as NCM source: {}", e);
            error!("{}\n{:?}", message, e);
            self.echoerr(&message)?;
        }

        Ok(result)
    }

    fn initialized(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::Initialized::METHOD);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        self.get_client(&Some(languageId))?
            .notify(lsp::notification::Initialized::METHOD, InitializedParams {})?;
        info!("End {}", lsp::notification::Initialized::METHOD);
        Ok(())
    }

    pub fn textDocument_hover(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::HoverRequest::METHOD);
        let (languageId, filename, line, character, handle): (String, String, u64, u64, bool) =
            self.gather_args(
                &[
                    VimVar::LanguageId,
                    VimVar::Filename,
                    VimVar::Line,
                    VimVar::Character,
                    VimVar::Handle,
                ],
                params,
            )?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::HoverRequest::METHOD,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position: Position { line, character },
            },
        )?;

        if !handle {
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
                self.echo_ellipsis(hover.to_string())?
            }
        }

        info!("End {}", lsp::request::HoverRequest::METHOD);
        Ok(result)
    }

    /// Generic find locations, e.g, definitions, references.
    pub fn find_locations(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        let (method,): (String,) = self.gather_args(&["method"], params)?;
        info!("Begin {}", method);
        let (languageId, filename, word, line, character, handle, goto_cmd): (
            String,
            String,
            String,
            u64,
            u64,
            bool,
            Option<String>,
        ) = self.gather_args(
            &[
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Cword,
                VimVar::Line,
                VimVar::Character,
                VimVar::Handle,
                VimVar::GotoCmd,
            ],
            params,
        )?;

        let params = serde_json::to_value(TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            position: Position { line, character },
        })?
        .combine(params);

        let result = self.get_client(&Some(languageId))?.call(&method, &params)?;

        if !handle {
            return Ok(result);
        }

        let response: Option<GotoDefinitionResponse> = result.clone().to_lsp()?;

        match response {
            None => {
                self.echowarn("Not found!")?;
                return Ok(Value::Null);
            }
            Some(GotoDefinitionResponse::Scalar(loc)) => {
                self.edit(&goto_cmd, loc.uri.filepath()?)?;
                self.cursor(loc.range.start.line + 1, loc.range.start.character + 1)?;
            }
            Some(GotoDefinitionResponse::Array(arr)) => match arr.len() {
                0 => self.echowarn("Not found!")?,
                1 => {
                    let loc = arr.get(0).ok_or_else(|| err_msg("Not found!"))?;
                    self.edit(&goto_cmd, loc.uri.filepath()?)?;
                    self.cursor(loc.range.start.line + 1, loc.range.start.character + 1)?;
                    let cur_file: String = self.eval("expand('%')")?;
                    self.echomsg_ellipsis(format!(
                        "{} {}:{}",
                        cur_file,
                        loc.range.start.line + 1,
                        loc.range.start.character + 1
                    ))?;
                }
                _ => {
                    let title = format!("[LC]: search for {}", word);
                    self.display_locations(&arr, &title)?
                }
            },
            Some(GotoDefinitionResponse::Link(_)) => {
                self.echowarn("Definition links are not supported!")?;
                return Ok(Value::Null);
            }
        };

        info!("End {}", method);
        Ok(result)
    }

    pub fn textDocument_rename(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::Rename::METHOD);
        let (languageId, filename, line, character, cword, new_name, handle): (
            String,
            String,
            u64,
            u64,
            String,
            Option<String>,
            bool,
        ) = self.gather_args(
            &[
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
                VimVar::Cword,
                VimVar::NewName,
                VimVar::Handle,
            ],
            params,
        )?;

        let mut new_name = new_name.unwrap_or_default();
        if new_name.is_empty() {
            let value = self
                .vim()?
                .call("s:getInput", ["Rename to: ".to_owned(), cword])?;
            new_name = serde_json::from_value(value)?;
        }
        if new_name.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::Rename::METHOD,
            RenameParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position: Position { line, character },
                new_name,
            },
        )?;

        if !handle || result == Value::Null {
            return Ok(result);
        }

        let edit: WorkspaceEdit = serde_json::from_value(result.clone())?;
        self.apply_WorkspaceEdit(&edit, params)?;

        info!("End {}", lsp::request::Rename::METHOD);
        Ok(result)
    }

    pub fn textDocument_documentSymbol(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::DocumentSymbolRequest::METHOD);

        let (languageId, filename, handle): (String, String, bool) = self.gather_args(
            &[VimVar::LanguageId, VimVar::Filename, VimVar::Handle],
            params,
        )?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::DocumentSymbolRequest::METHOD,
            DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
            },
        )?;

        if !handle {
            return Ok(result);
        }

        let symbols: Vec<SymbolInformation> = serde_json::from_value(result.clone())?;
        let title = format!("[LC]: symbols for {}", filename);

        let selectionUI = self.get(|state| state.selectionUI)?;
        let selectionUI_autoOpen = self.get(|state| state.selectionUI_autoOpen)?;
        match selectionUI {
            SelectionUI::FZF => {
                let source: Vec<_> = symbols
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
                    .collect();

                self.vim()?.notify(
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Fallible<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.setqflist(&list, " ", &title)?;
                if selectionUI_autoOpen {
                    self.command("botright copen")?;
                }
                self.echo("Document symbols populated to quickfix list.")?;
            }
            SelectionUI::LocationList => {
                let list: Fallible<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.setloclist(&list, " ", &title)?;
                if selectionUI_autoOpen {
                    self.command("lopen")?;
                }
                self.echo("Document symbols populated to location list.")?;
            }
        }

        info!("End {}", lsp::request::DocumentSymbolRequest::METHOD);
        Ok(result)
    }

    pub fn textDocument_codeAction(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::CodeActionRequest::METHOD);
        let (languageId, filename, line, character, handle): (String, String, u64, u64, bool) =
            self.gather_args(
                &[
                    VimVar::LanguageId,
                    VimVar::Filename,
                    VimVar::Line,
                    VimVar::Character,
                    VimVar::Handle,
                ],
                params,
            )?;

        // Unify filename.
        let filename = filename.canonicalize();

        let diagnostics: Vec<_> = self.get(|state| {
            state
                .diagnostics
                .get(&filename)
                .unwrap_or(&vec![])
                .iter()
                .filter(|dn| {
                    let start = dn.range.start;
                    let end = dn.range.end;
                    (line, character) >= (start.line, start.character)
                        && (line, character) < (end.line, end.character)
                })
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
                    start: Position { line, character },
                    end: Position { line, character },
                },
                context: CodeActionContext {
                    diagnostics,
                    only: None,
                },
            },
        )?;

        let commands: Vec<Command> = serde_json::from_value(result.clone())?;

        let source: Vec<_> = commands
            .iter()
            .map(|cmd| format!("{}: {}", cmd.command, cmd.title))
            .collect();

        self.update(|state| {
            state.stashed_codeAction_commands = commands;
            Ok(())
        })?;

        if !handle {
            return Ok(result);
        }

        self.vim()?
            .notify("s:FZF", json!([source, NOTIFICATION__FZFSinkCommand]))?;

        info!("End {}", lsp::request::CodeActionRequest::METHOD);
        Ok(result)
    }

    pub fn textDocument_completion(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::Completion::METHOD);

        let (languageId, filename, line, character, handle): (String, String, u64, u64, bool) =
            self.gather_args(
                &[
                    VimVar::LanguageId,
                    VimVar::Filename,
                    VimVar::Line,
                    VimVar::Character,
                    VimVar::Handle,
                ],
                params,
            )?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::Completion::METHOD,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position: Position { line, character },
            },
        )?;

        if !handle {
            return Ok(result);
        }

        info!("End {}", lsp::request::Completion::METHOD);
        Ok(result)
    }

    pub fn textDocument_signatureHelp(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::SignatureHelpRequest::METHOD);
        let (languageId, filename, line, character, handle): (String, String, u64, u64, bool) =
            self.gather_args(
                &[
                    VimVar::LanguageId,
                    VimVar::Filename,
                    VimVar::Line,
                    VimVar::Character,
                    VimVar::Handle,
                ],
                params,
            )?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::SignatureHelpRequest::METHOD,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position: Position { line, character },
            },
        )?;

        if !handle || result == Value::Null {
            return Ok(result);
        }

        let help: SignatureHelp = serde_json::from_value(result)?;
        if help.signatures.is_empty() {
            return Ok(Value::Null);
        }
        let active_signature = help
            .signatures
            .get(help.active_signature.unwrap_or(0).to_usize()?)
            .ok_or_else(|| err_msg("Failed to get active signature"))?;
        let active_parameter: Option<&ParameterInformation>;
        if let Some(ref parameters) = active_signature.parameters {
            active_parameter = parameters.get(help.active_parameter.unwrap_or(0).to_usize()?);
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
            self.command(&cmd)?;
        } else {
            self.echo(&active_signature.label)?;
        }

        info!("End {}", lsp::request::SignatureHelpRequest::METHOD);
        Ok(Value::Null)
    }

    pub fn textDocument_references(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::References::METHOD);

        let (include_declaration,): (bool,) =
            self.gather_args(&[VimVar::IncludeDeclaration], params)?;

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
        let (languageId, filename, handle): (String, String, bool) = self.gather_args(
            &[VimVar::LanguageId, VimVar::Filename, VimVar::Handle],
            params,
        )?;

        let (tab_size, insert_spaces): (u64, u64) =
            self.eval(["shiftwidth()", "&expandtab"].as_ref())?;
        let insert_spaces = insert_spaces == 1;
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
                },
            },
        )?;

        if !handle {
            return Ok(result);
        }

        let text_edits: Option<Vec<TextEdit>> = serde_json::from_value(result.clone())?;
        let text_edits = text_edits.unwrap_or_default();
        let edit = lsp::WorkspaceEdit {
            changes: Some(hashmap! {filename.to_url()? => text_edits}),
            document_changes: None,
        };
        self.apply_WorkspaceEdit(&edit, params)?;
        info!("End {}", lsp::request::Formatting::METHOD);
        Ok(result)
    }

    pub fn textDocument_rangeFormatting(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::RangeFormatting::METHOD);
        let (languageId, filename, handle, start_line, end_line): (String, String, bool, u64, u64) =
            self.gather_args(
                &[
                    VimVar::LanguageId.to_key().as_str(),
                    VimVar::Filename.to_key().as_str(),
                    VimVar::Handle.to_key().as_str(),
                    "LSP#range_start_line()",
                    "LSP#range_end_line()",
                ],
                params,
            )?;

        let (tab_size, insert_spaces): (u64, u64) =
            self.eval(["shiftwidth()", "&expandtab"].as_ref())?;
        let insert_spaces = insert_spaces == 1;
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
            },
        )?;

        if !handle {
            return Ok(result);
        }

        let text_edits: Option<Vec<TextEdit>> = serde_json::from_value(result.clone())?;
        let text_edits = text_edits.unwrap_or_default();
        let edit = lsp::WorkspaceEdit {
            changes: Some(hashmap! {filename.to_url()? => text_edits}),
            document_changes: None,
        };
        self.apply_WorkspaceEdit(&edit, params)?;
        info!("End {}", lsp::request::RangeFormatting::METHOD);
        Ok(result)
    }

    pub fn completionItem_resolve(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::ResolveCompletionItem::METHOD);
        let (languageId, handle): (String, bool) =
            self.gather_args(&[VimVar::LanguageId, VimVar::Handle], params)?;
        let (completion_item,): (CompletionItem,) =
            self.gather_args(&["completionItem"], params)?;

        let result = self
            .get_client(&Some(languageId))?
            .call(lsp::request::ResolveCompletionItem::METHOD, completion_item)?;

        if !handle {
            return Ok(result);
        }

        // TODO: proper integration.
        let msg = format!("comletionItem/resolve result not handled: {:?}", result);
        warn!("{}", msg);
        self.echowarn(&msg)?;

        info!("End {}", lsp::request::ResolveCompletionItem::METHOD);
        Ok(Value::Null)
    }

    pub fn workspace_symbol(&self, params: &Value) -> Fallible<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::WorkspaceSymbol::METHOD);
        let (languageId, handle): (String, bool) =
            self.gather_args(&[VimVar::LanguageId, VimVar::Handle], params)?;

        let (query,): (String,) = self.gather_args(&[("query", "")], params)?;
        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::WorkspaceSymbol::METHOD,
            WorkspaceSymbolParams { query },
        )?;

        if !handle {
            return Ok(result);
        }

        let symbols: Vec<SymbolInformation> = serde_json::from_value(result.clone())?;
        let title = "[LC]: workspace symbols";

        let selectionUI = self.get(|state| state.selectionUI)?;
        let selectionUI_autoOpen = self.get(|state| state.selectionUI_autoOpen)?;
        match selectionUI {
            SelectionUI::FZF => {
                let cwd: String = self.eval("getcwd()")?;
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

                self.vim()?.notify(
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Fallible<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.setqflist(&list, " ", title)?;
                if selectionUI_autoOpen {
                    self.command("botright copen")?;
                }
                self.echo("Workspace symbols populated to quickfix list.")?;
            }
            SelectionUI::LocationList => {
                let list: Fallible<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.setloclist(&list, " ", title)?;
                if selectionUI_autoOpen {
                    self.command("lopen")?;
                }
                self.echo("Workspace symbols populated to location list.")?;
            }
        }

        info!("End {}", lsp::request::WorkspaceSymbol::METHOD);
        Ok(result)
    }

    pub fn workspace_executeCommand(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::ExecuteCommand::METHOD);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let (command, arguments): (String, Vec<Value>) =
            self.gather_args(&["command", "arguments"], params)?;

        let result = self.get_client(&Some(languageId))?.call(
            lsp::request::ExecuteCommand::METHOD,
            ExecuteCommandParams { command, arguments },
        )?;
        info!("End {}", lsp::request::ExecuteCommand::METHOD);
        Ok(result)
    }

    pub fn workspace_applyEdit(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", lsp::request::ApplyWorkspaceEdit::METHOD);

        let params: ApplyWorkspaceEditParams = params.clone().to_lsp()?;
        self.apply_WorkspaceEdit(&params.edit, &Value::Null)?;

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
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let (settings,): (Value,) = self.gather_args(&["settings"], params)?;

        self.get_client(&Some(languageId))?.notify(
            lsp::notification::DidChangeConfiguration::METHOD,
            DidChangeConfigurationParams { settings },
        )?;
        info!("End {}", lsp::notification::DidChangeConfiguration::METHOD);
        Ok(())
    }

    pub fn textDocument_didOpen(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidOpenTextDocument::METHOD);
        let (languageId, filename, text): (String, String, Vec<String>) = self.gather_args(
            &[VimVar::LanguageId, VimVar::Filename, VimVar::Text],
            params,
        )?;

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

        self.command("setlocal omnifunc=LanguageClient#complete")?;
        let root = self.get(|state| state.roots.get(&languageId).cloned().unwrap_or_default())?;
        self.vim()?.notify(
            "setbufvar",
            json!([filename, "LanguageClient_projectRoot", root]),
        )?;
        self.vim()?
            .notify("s:ExecuteAutocmd", "LanguageClientTextDocumentDidOpenPost")?;

        info!("End {}", lsp::notification::DidOpenTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_didChange(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidChangeTextDocument::METHOD);
        let (bufnr, languageId, filename): (u64, String, String) = self.gather_args(
            &[VimVar::Bufnr, VimVar::LanguageId, VimVar::Filename],
            params,
        )?;
        if !self.get(|state| state.text_documents.contains_key(&filename))? {
            info!("Not opened yet. Switching to didOpen.");
            return self.textDocument_didOpen(params);
        }

        let (text,): (Vec<String>,) =
            self.gather_args(&[format!("LSP#text({})", bufnr)], params)?;

        let text = text.join("\n");
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

        info!("End {}", lsp::notification::DidChangeTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_didSave(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidSaveTextDocument::METHOD);
        let (languageId, filename): (String, String) =
            self.gather_args(&[VimVar::LanguageId, VimVar::Filename], params)?;
        if !self.get(|state| state.serverCommands.contains_key(&languageId))? {
            return Ok(());
        }

        let uri = filename.to_url()?;

        self.get_client(&Some(languageId))?.notify(
            lsp::notification::DidSaveTextDocument::METHOD,
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
            },
        )?;

        info!("End {}", lsp::notification::DidSaveTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_didClose(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::DidCloseTextDocument::METHOD);
        let (languageId, filename): (String, String) =
            self.gather_args(&[VimVar::LanguageId, VimVar::Filename], params)?;

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
        if filename.chars().nth(0) == Some('/') && filename.chars().nth(2) == Some(':') {
            filename.remove(0);
        }
        // Unify name to avoid mismatch due to case insensitivity.
        let filename = filename.canonicalize();

        let mut diagnostics = params.diagnostics;
        diagnostics.sort_by_key(
            // First sort by line.
            // Then severity descendingly. Error should come last since when processing item comes
            // later will override its precedance.
            // Then by character descendingly.
            |diagnostic| {
                (
                    diagnostic.range.start.line,
                    -(diagnostic.severity.unwrap_or(DiagnosticSeverity::Hint) as i8),
                    -(diagnostic.range.start.line as i64),
                )
            },
        );

        self.update(|state| {
            state
                .diagnostics
                .insert(filename.clone(), diagnostics.clone());
            Ok(())
        })?;
        self.update_quickfixlist()?;

        let current_filename: String = self.eval(VimVar::Filename)?;
        if filename != current_filename.canonicalize() {
            return Ok(());
        }
        self.process_diagnostics(&current_filename, &diagnostics)?;
        self.languageClient_handleCursorMoved(&Value::Null)?;
        self.vim()?
            .notify("s:ExecuteAutocmd", "LanguageClientDiagnosticsChanged")?;

        info!("End {}", lsp::notification::PublishDiagnostics::METHOD);
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
        self.echomsg(&msg)?;
        info!("End {}", lsp::notification::LogMessage::METHOD);
        Ok(())
    }

    pub fn window_showMessage(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::ShowMessage::METHOD);
        let params: ShowMessageParams = params.clone().to_lsp()?;
        let msg = format!("[{:?}] {}", params.typ, params.message);
        self.echomsg(&msg)?;
        info!("End {}", lsp::notification::ShowMessage::METHOD);
        Ok(())
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
                                watcher.watch(
                                    w.glob_pattern.trim_right_matches("**"),
                                    recursive_mode,
                                )?;
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
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;

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
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let is_alive = self.get(|state| state.clients.contains_key(&Some(languageId)))?;
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
        self.command(&exp)?;
        info!("End {}", REQUEST__RegisterServerCommands);
        Ok(Value::Null)
    }

    pub fn languageClient_setLoggingLevel(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__SetLoggingLevel);
        let (loggingLevel,): (log::LevelFilter,) = self.gather_args(&["loggingLevel"], params)?;
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
        let (diagnosticsList,): (DiagnosticsList,) =
            self.gather_args(&["diagnosticsList"], params)?;
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
                if **k == VimVar::Bufnr.to_key() || **k == VimVar::LanguageId.to_key() {
                    return None;
                }

                Some(match serde_json::to_string(v) {
                    Ok(v) => Ok((k.clone(), v)),
                    Err(err) => Err(err.into()),
                })
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
        let mut matches = match result {
            CompletionResponse::Array(arr) => arr,
            CompletionResponse::List(list) => list.items,
        };
        if !matches.iter().any(|m| m.sort_text.is_none()) {
            matches.sort_by(|m1, m2| {
                m1.sort_text
                    .as_ref()
                    .unwrap()
                    .cmp(m2.sort_text.as_ref().unwrap())
            });
        }

        let (complete_position,): (Option<u64>,) =
            self.gather_args(&[("complete_position", "v:null")], params)?;

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
        let (filename,): (String,) = self.gather_args(&[VimVar::Filename], params)?;
        if filename.is_empty() {
            return Ok(());
        }
        let autoStart: u8 = self.eval("!!get(g:, 'LanguageClient_autoStart', 1)")?;
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
        let (languageId, filename): (String, String) =
            self.gather_args(&[VimVar::LanguageId, VimVar::Filename], params)?;
        if filename.is_empty() {
            return Ok(());
        }

        let filename = filename.canonicalize();

        if self.get(|state| state.clients.contains_key(&Some(languageId.clone())))? {
            self.textDocument_didOpen(params)?;

            if let Some(diagnostics) =
                self.get(|state| state.diagnostics.get(&filename).cloned())?
            {
                self.process_diagnostics(&filename, &diagnostics)?;
                self.languageClient_handleCursorMoved(params)?;
            }
        } else {
            let autoStart: u8 = self.eval("!!get(g:, 'LanguageClient_autoStart', 1)")?;
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
        let (languageId, filename): (String, String) =
            self.gather_args(&[VimVar::LanguageId, VimVar::Filename], params)?;
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
        let (languageId, filename): (String, String) =
            self.gather_args(&[VimVar::LanguageId, VimVar::Filename], params)?;
        if !self.get(|state| state.serverCommands.contains_key(&languageId))? {
            return Ok(());
        }

        self.update(|state| {
            state.text_documents.retain(|f, _| f != &filename);
            state.diagnostics.retain(|f, _| f != &filename);
            state.line_diagnostics.retain(|fl, _| fl.0 != filename);
            state.signs.retain(|f, _| f != &filename);
            Ok(())
        })?;
        self.textDocument_didClose(params)?;
        info!("End {}", NOTIFICATION__HandleBufWritePost);
        Ok(())
    }

    pub fn languageClient_handleCursorMoved(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__HandleCursorMoved);
        let (languageId, filename, bufnr, line): (String, String, i64, u64) = self.gather_args(
            &[
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Bufnr,
                VimVar::Line,
            ],
            params,
        )?;
        if !self.get(|state| state.serverCommands.contains_key(&languageId))? {
            return Ok(());
        }

        let (visible_line_start, visible_line_end): (u64, u64) = self.gather_args(
            &["LSP#visible_line_start()", "LSP#visible_line_end()"],
            params,
        )?;
        if !self.get(|state| state.diagnostics.contains_key(&filename))? {
            return Ok(());
        }

        if line != self.get(|state| state.last_cursor_line)? {
            self.update(|state| {
                state.last_cursor_line = line;
                Ok(())
            })?;

            let message = self.get(|state| {
                state
                    .line_diagnostics
                    .get(&(filename.clone(), line))
                    .cloned()
                    .unwrap_or_default()
            })?;

            if message != self.get(|state| state.last_line_diagnostic.clone())? {
                self.echo_ellipsis(&message)?;
                self.update(|state| {
                    state.last_line_diagnostic = message;
                    Ok(())
                })?;
            }
        }

        let signs: Vec<_> = self.update(|state| {
            Ok(state
                .signs
                .entry(filename.clone())
                .or_insert_with(|| vec![])
                .iter()
                .filter_map(|s| {
                    if s.line < visible_line_start + 1 || s.line > visible_line_end + 1 {
                        return None;
                    }

                    Some(s.clone())
                })
                .collect())
        })?;
        let signs_prev = self.get(|state| {
            state
                .signs_placed
                .get(&filename)
                .cloned()
                .unwrap_or_default()
        })?;
        if signs != signs_prev {
            let (signs, cmds) = get_command_update_signs(&signs_prev, &signs, &filename);
            self.update(|state| {
                state.signs_placed.insert(filename.clone(), signs);
                Ok(())
            })?;

            info!("Updating signs: {:?}", cmds);
            self.command(&cmds)?;
        }

        let highlights: Vec<_> = self.update(|state| {
            Ok(state
                .highlights
                .entry(filename.clone())
                .or_insert_with(|| vec![])
                .iter()
                .filter_map(|h| {
                    if h.line < visible_line_start || h.line > visible_line_end {
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

            self.vim()?.notify(
                "nvim_buf_clear_highlight",
                json!([0, source, visible_line_start, visible_line_end]),
            )?;

            self.vim()?
                .notify("s:AddHighlights", json!([source, highlights]))?;
        }

        if self.get(|state| state.use_virtual_text)? {
            let namespace_id = if let Some(namespace_id) = self.get(|state| state.namespace_id)? {
                namespace_id
            } else {
                let namespace_id = self.create_namespace("LanguageClient")?;
                self.update(|state| {
                    state.namespace_id = Some(namespace_id);
                    Ok(())
                })?;
                namespace_id
            };
            let mut virtual_texts = vec![];
            self.update(|state| {
                if let Some(diagnostics) = state.diagnostics.get(&filename) {
                    for diagnostic in diagnostics {
                        if diagnostic.range.start.line >= visible_line_start
                            && diagnostic.range.start.line <= visible_line_end
                        {
                            virtual_texts.push(VirtualText {
                                line: diagnostic.range.start.line,
                                text: diagnostic.message.clone(),
                                hl_group: state
                                    .diagnosticsDisplay
                                    .get(
                                        &diagnostic
                                            .severity
                                            .unwrap_or(DiagnosticSeverity::Hint)
                                            .to_int()?,
                                    )
                                    .ok_or_else(|| err_msg("Failed to get display"))?
                                    .virtualTexthl
                                    .clone(),
                            });
                        }
                    }
                }
                Ok(())
            })?;
            self.set_virtual_texts(
                bufnr,
                namespace_id,
                visible_line_start,
                visible_line_end,
                &virtual_texts,
            )?;
        }

        info!("End {}", NOTIFICATION__HandleCursorMoved);
        Ok(())
    }

    pub fn languageClient_handleCompleteDone(&self, params: &Value) -> Fallible<()> {
        let (filename, completed_item, line, character): (String, VimCompleteItem, u64, u64) = self
            .gather_args(
                &[
                    VimVar::Filename.to_key().as_str(),
                    "completed_item",
                    VimVar::Line.to_key().as_str(),
                    VimVar::Character.to_key().as_str(),
                ],
                params,
            )?;

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
                self.command("undo")?;
                edits.push(edit.clone());
            };
        }
        if let Some(aedits) = lspitem.additional_text_edits {
            edits.extend(aedits.clone());
        };

        if edits.is_empty() {
            return Ok(());
        }

        self.apply_TextEdits(filename, &edits)?;
        self.cursor(line + 1, character + 1)
    }

    pub fn languageClient_FZFSinkLocation(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__FZFSinkLocation);
        let params = match params {
            Value::Array(ref arr) => Value::Array(arr.clone()),
            _ => {
                bail!("Expecting array params!");
            }
        };

        let lines: Vec<String> = serde_json::from_value(params)?;
        if lines.is_empty() {
            err_msg("No selection!");
        }

        let location = lines
            .get(0)
            .ok_or_else(|| format_err!("Failed to get line! lines: {:?}", lines))?
            .split('\t')
            .next()
            .ok_or_else(|| format_err!("Failed to parse: {:?}", lines))?;
        let mut tokens: Vec<_> = location.split_terminator(':').collect();
        tokens.reverse();
        let filename: String = if tokens.len() > 2 {
            let relpath = tokens
                .pop()
                .ok_or_else(|| format_err!("Failed to get file path! tokens: {:?}", tokens))?
                .to_owned();
            let cwd: String = self.eval("getcwd()")?;
            Path::new(&cwd).join(relpath).to_string_lossy().into_owned()
        } else {
            self.eval(VimVar::Filename)?
        };
        let line = tokens
            .pop()
            .ok_or_else(|| format_err!("Failed to get line! tokens: {:?}", tokens))?
            .to_int()?
            - 1;
        let character = tokens
            .pop()
            .ok_or_else(|| format_err!("Failed to get character! tokens: {:?}", tokens))?
            .to_int()?
            - 1;

        self.edit(&None, &filename)?;
        self.cursor(line + 1, character + 1)?;

        info!("End {}", NOTIFICATION__FZFSinkLocation);
        Ok(())
    }

    pub fn languageClient_FZFSinkCommand(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__FZFSinkCommand);
        let (selection,): (String,) = self.gather_args(&["selection"], params)?;
        let tokens: Vec<&str> = selection.splitn(2, ": ").collect();
        let command = tokens
            .get(0)
            .cloned()
            .ok_or_else(|| format_err!("Failed to get command! tokens: {:?}", tokens))?;
        let title = tokens
            .get(1)
            .cloned()
            .ok_or_else(|| format_err!("Failed to get title! tokens: {:?}", tokens))?;
        let entry = self.get(|state| {
            let commands = &state.stashed_codeAction_commands;

            commands
                .iter()
                .find(|e| e.command == command && e.title == title)
                .cloned()
                .ok_or_else(|| {
                    format_err!("No stashed command found! stashed commands: {:?}", commands)
                })
        })??;

        if self.try_handle_command_by_client(&entry)? {
            return Ok(());
        }

        self.workspace_executeCommand(&json!({
            "command": entry.command,
            "arguments": entry.arguments,
        }))?;

        self.update(|state| {
            state.stashed_codeAction_commands = vec![];
            Ok(())
        })?;

        info!("End {}", NOTIFICATION__FZFSinkCommand);
        Ok(())
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
        self.vim()?.notify(
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
        self.vim()?.notify(
            "ncm2#complete",
            json!([orig_ctx, ctx.startccol, matches, is_incomplete]),
        )?;
        info!("End {}", REQUEST__NCM2OnComplete);
        result
    }

    pub fn languageClient_explainErrorAtPoint(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__ExplainErrorAtPoint);
        let (filename, line, character): (String, u64, u64) =
            self.gather_args(&[VimVar::Filename, VimVar::Line, VimVar::Character], params)?;
        let diag = self.get(|state| {
            state
                .diagnostics
                .get(&filename)
                .ok_or_else(|| format_err!("No diagnostics found: filename: {}", filename,))?
                .iter()
                .find(|d| {
                    (line, character) >= (d.range.start.line, d.range.start.character)
                        && (line, character) < (d.range.end.line, d.range.end.character)
                })
                .cloned()
                .ok_or_else(|| {
                    format_err!(
                        "No diagnostics found: filename: {}, line: {}, character: {}",
                        filename,
                        line,
                        character
                    )
                })
        })??;
        self.preview(diag.message.as_str())?;

        info!("End {}", REQUEST__ExplainErrorAtPoint);
        Ok(Value::Null)
    }

    // Extensions by language servers.
    pub fn language_status(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__LanguageStatus);
        let params: LanguageStatusParams = params.clone().to_lsp()?;
        let msg = format!("{} {}", params.typee, params.message);
        self.echomsg(&msg)?;
        info!("End {}", NOTIFICATION__LanguageStatus);
        Ok(())
    }

    pub fn rust_handleBeginBuild(&self, _params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__RustBeginBuild);
        self.command(vec![
            format!("let {}=1", VIM__ServerStatus),
            format!("let {}='Rust: build begin'", VIM__ServerStatusMessage),
        ])?;
        info!("End {}", NOTIFICATION__RustBeginBuild);
        Ok(())
    }

    pub fn rust_handleDiagnosticsBegin(&self, _params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__RustDiagnosticsBegin);
        self.command(vec![
            format!("let {}=1", VIM__ServerStatus),
            format!("let {}='Rust: diagnostics begin'", VIM__ServerStatusMessage),
        ])?;
        info!("End {}", NOTIFICATION__RustDiagnosticsBegin);
        Ok(())
    }

    pub fn rust_handleDiagnosticsEnd(&self, _params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__RustDiagnosticsEnd);
        self.command(vec![
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
            buf += params
                .title
                .as_ref()
                .map(|title| title.as_ref())
                .unwrap_or("Busy");

            // For RLS this is the crate name, present only if the progress isn't known.
            if let Some(message) = params.message {
                buf += &format!(" ({})", &message);
            }
            // For RLS this is the progress percentage, present only if the it's known.
            if let Some(percentage) = params.percentage {
                buf += &format!(" ({:.1}% done)", percentage);
            }
        }

        self.command(vec![
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
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], &params)?;
        let (cmdargs,): (Vec<String>,) = self.gather_args(&[("cmdargs", "[]")], params)?;
        let cmdparams = vim_cmd_args_to_value(&cmdargs)?;
        let params = params.combine(&cmdparams);
        let (filename,): (String,) = self.gather_args(&[VimVar::Filename], &params)?;

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

        let (rootPath,): (Option<String>,) =
            self.gather_args(&[("rootPath", "v:null")], &params)?;
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
        self.echomsg_ellipsis(&message)?;
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
                VimVar::LanguageId.to_key(): languageId,
                "settings": settings,
            }))?,
            Err(err) => warn!("Failed to get workspace settings: {}", err),
        }

        self.textDocument_didOpen(&params)?;
        self.textDocument_didChange(&params)?;

        self.vim()?
            .notify("s:ExecuteAutocmd", "LanguageClientStarted")?;
        Ok(Value::Null)
    }

    pub fn languageClient_serverExited(&self, params: &Value) -> Fallible<()> {
        let (languageId, message): (String, String) = self.gather_args(
            [VimVar::LanguageId.to_key().as_str(), "message"].as_ref(),
            params,
        )?;

        if self.get(|state| state.clients.contains_key(&Some(languageId.clone())))? {
            if let Err(err) = self.cleanup(&languageId) {
                error!("Error in cleanup: {:?}", err);
            }
            if let Err(err) = self.echoerr(format!(
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
        let (languageId,): (String,) = self.gather_args([VimVar::LanguageId].as_ref(), params)?;

        let params: DidChangeWatchedFilesParams = params.clone().to_lsp()?;
        self.get_client(&Some(languageId))?
            .notify(lsp::notification::DidChangeWatchedFiles::METHOD, params)?;

        info!("End {}", lsp::notification::DidChangeWatchedFiles::METHOD);
        Ok(())
    }

    pub fn java_classFileContents(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__ClassFileContents);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;

        let content: String = self
            .get_client(&Some(languageId))?
            .call(REQUEST__ClassFileContents, params)?;

        info!("End {}", REQUEST__ClassFileContents);
        Ok(Value::String(content))
    }

    pub fn debug_info(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__DebugInfo);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
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
                    .get(&Some(languageId))
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
        self.echo(&msg)?;
        info!("End {}", REQUEST__DebugInfo);
        Ok(json!(msg))
    }
}
