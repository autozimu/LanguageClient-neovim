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

    pub fn loop_call(&self, rx: &crossbeam_channel::Receiver<Call>) -> Fallible<()> {
        for call in rx.iter() {
            let language_client = Self(self.0.clone());
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

        let (
            diagnosticsSignsMax,
            documentHighlightDisplay,
            selectionUI_autoOpen,
            use_virtual_text,
            echo_project_root,
        ): (Option<u64>, Value, u8, u8, u8) = self.vim()?.eval(
            [
                "get(g:, 'LanguageClient_diagnosticsSignsMax', v:null)",
                "get(g:, 'LanguageClient_documentHighlightDisplay', {})",
                "!!s:GetVar('LanguageClient_selectionUI_autoOpen', 1)",
                "s:useVirtualText()",
                "!!s:GetVar('LanguageClient_echoProjectRoot', 1)",
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
            state.echo_project_root = echo_project_root == 1;
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

        self.vim()?.command(cmds)?;
        Ok(())
    }

    fn apply_WorkspaceEdit(&self, edit: &WorkspaceEdit) -> Fallible<()> {
        debug!("Begin apply WorkspaceEdit: {:?}", edit);
        let filename = self.vim()?.get_filename(&Value::Null)?;
        let position = self.vim()?.get_position(&Value::Null)?;

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
        } else if let Some(ref changes) = edit.changes {
            for (uri, edits) in changes {
                self.apply_TextEdits(&uri.filepath()?, edits)?;
            }
        }
        self.vim()?.edit(&None, &filename)?;
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

        let result = self.get_client(&Some(languageId.clone()))?.call(
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

            let source = if let Some(hs) = self.get(|state| state.document_highlight_source)? {
                if hs.buffer == buffer {
                    // If we want to highlight in the same buffer as last time, we can reuse
                    // the previous source.
                    Some(hs.source)
                } else {
                    // Clear the highlight in the previous buffer.
                    self.vim()?.rpcclient.notify(
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
                    let source = self.vim()?.rpcclient.call(
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
                .rpcclient
                .notify("nvim_buf_clear_highlight", json!([buffer, source, 0, -1]))?;
            self.vim()?
                .rpcclient
                .notify("s:AddHighlights", json!([source, highlights]))?;
        }

        info!("End {}", lsp::request::DocumentHighlightRequest::METHOD);
        Ok(result)
    }

    pub fn languageClient_clearDocumentHighlight(&self, _params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__ClearDocumentHighlight);

        let buffer_source = self.update(|state| Ok(state.document_highlight_source.take()))?;
        if let Some(HighlightSource { buffer, source }) = buffer_source {
            self.vim()?
                .rpcclient
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

        self.vim()?.edit(&None, path)?;

        let mut lines: Vec<String> = self.vim()?.rpcclient.call("getline", json!([1, '$']))?;
        let lines_len_prev = lines.len();
        let fixendofline = self.vim()?.eval::<_, u8>("&fixendofline")? == 1;
        if lines.last().map(String::is_empty) == Some(false) && fixendofline {
            lines.push("".to_owned());
        }

        let mut lines = apply_TextEdits(&lines, &edits)?;

        if lines.last().map(String::is_empty) == Some(true) && fixendofline {
            lines.pop();
        }
        if lines.len() < lines_len_prev {
            self.vim()?
                .command(format!("{},{}d", lines.len() + 1, lines_len_prev))?;
        }
        self.vim()?.rpcclient.notify("setline", json!([1, lines]))?;
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
                            middleLines.push(endLine);
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
        if !CommandsClient.contains(&cmd.command.as_str()) {
            return Ok(false);
        }

        if cmd.command == "java.apply.workspaceEdit" {
            if let Some(ref edits) = cmd.arguments {
                for edit in edits {
                    let edit: WorkspaceEdit = serde_json::from_value(edit.clone())?;
                    self.apply_WorkspaceEdit(&edit)?;
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
            self.vim()?.echoerr(&message)?;
        }
        if let Err(e) = self.registerNCM2Source(&languageId, &result) {
            let message = format!("LanguageClient: failed to register as NCM source: {}", e);
            error!("{}\n{:?}", message, e);
            self.vim()?.echoerr(&message)?;
        }

        Ok(result)
    }

    fn initialized(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", lsp::notification::Initialized::METHOD);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        self.get_client(&Some(languageId.clone()))?
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

        let result = self.get_client(&Some(languageId.clone()))?.call(
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

        let result = self
            .get_client(&Some(languageId.clone()))?
            .call(&method, &params)?;

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
                self.vim()?.edit(&goto_cmd, loc.uri.filepath()?)?;
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

        let result = self.get_client(&Some(languageId.clone()))?.call(
            lsp::request::Rename::METHOD,
            RenameParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position,
                new_name,
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

        let result = self.get_client(&Some(languageId.clone()))?.call(
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

                self.vim()?.rpcclient.notify(
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Fallible<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.vim()?.setqflist(&list, " ", &title)?;
                if selectionUI_autoOpen {
                    self.vim()?.command("botright copen")?;
                }
                self.vim()?
                    .echo("Document symbols populated to quickfix list.")?;
            }
            SelectionUI::LocationList => {
                let list: Fallible<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
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

        let result: Value = self.get_client(&Some(languageId.clone()))?.call(
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

        let result = self.get_client(&Some(languageId.clone()))?.call(
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

        let result = self.get_client(&Some(languageId.clone()))?.call(
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
        let result = self.get_client(&Some(languageId.clone()))?.call(
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
        let result = self.get_client(&Some(languageId.clone()))?.call(
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
            .get_client(&Some(languageId.clone()))?
            .call(lsp::request::ResolveCompletionItem::METHOD, completion_item)?;

        if !self.vim()?.get_handle(params)? {
            return Ok(result);
        }

        // TODO: proper integration.
        let msg = format!("comletionItem/resolve result not handled: {:?}", result);
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
        let result = self.get_client(&Some(languageId.clone()))?.call(
            lsp::request::WorkspaceSymbol::METHOD,
            WorkspaceSymbolParams { query },
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
        let arguments: Vec<Value> = try_get("arguments", params)?
            .ok_or_else(|| err_msg("argument not found in request!"))?;

        let result = self.get_client(&Some(languageId.clone()))?.call(
            lsp::request::ExecuteCommand::METHOD,
            ExecuteCommandParams { command, arguments },
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

        self.get_client(&Some(languageId.clone()))?.notify(
            lsp::notification::DidChangeConfiguration::METHOD,
            DidChangeConfigurationParams { settings },
        )?;
        info!("End {}", lsp::notification::DidChangeConfiguration::METHOD);
        Ok(())
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

        self.get_client(&Some(languageId.clone()))?.notify(
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
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;
        if !self.get(|state| state.serverCommands.contains_key(&languageId))? {
            return Ok(());
        }

        let uri = filename.to_url()?;

        self.get_client(&Some(languageId.clone()))?.notify(
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
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        self.get_client(&Some(languageId.clone()))?.notify(
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

        let current_filename: String = self.vim()?.get_filename(&Value::Null)?;
        if filename != current_filename.canonicalize() {
            return Ok(());
        }
        self.process_diagnostics(&current_filename, &diagnostics)?;
        self.update(|state| {
            state.viewports.remove(&filename);
            Ok(())
        })?;
        self.languageClient_handleCursorMoved(&Value::Null)?;
        self.vim()?
            .rpcclient
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
        let msg_params: ShowMessageRequestParams = params.clone().to_lsp()?;
        let msg_actions = msg_params.actions.unwrap_or_default();
        let mut options = Vec::with_capacity(msg_actions.len() + 1);
        options.push(msg_params.message);
        options.extend(
            msg_actions
                .iter()
                .enumerate()
                .map(|(i, item)| format!("{}) {}", i + 1, item.title)),
        );

        let mut v = Value::Null;
        let index: Option<usize> = self.vim()?.rpcclient.call("s:inputlist", options)?;
        if let Some(index) = index {
            v = serde_json::to_value(msg_actions.get(index - 1))?;
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
        if !self.get(|state| state.diagnostics.contains_key(&filename))? {
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
                .group_by(|(line, _)| *line)
                .into_iter()
                .filter_map(|(_, group)| group.min_by_key(|(_, severity)| *severity))
                .take(limit)
                .map(|(line, severity)| Sign::new(line, format!("LanguageClient{:?}", severity)))
                .collect())
        })?;
        let signs_prev: Vec<_> = self.update(|state| {
            Ok(state
                .signs
                .entry(filename.clone())
                .or_default()
                .iter()
                .map(|(_, sign)| sign.clone())
                .collect())
        })?;
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
                sign.id = self.update(|state| {
                    state.sign_next_id += 1;
                    Ok(state.sign_next_id)
                })?;
            }
        }
        self.vim()?
            .set_signs(&filename, &signs_to_add, &signs_to_delete)?;
        self.update(|state| {
            let signs = state.signs.entry(filename.clone()).or_default();
            // signs might be deleted AND added in the same line to change severity,
            // so deletions must be before additions
            for sign in signs_to_delete {
                signs.remove(&sign.line);
            }
            for sign in signs_to_add {
                signs.insert(sign.line, sign);
            }
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

        if self.get(|state| state.use_virtual_text)? {
            let namespace_id = self.get_or_create_namespace()?;

            let mut virtual_texts = vec![];
            self.update(|state| {
                if let Some(diag_list) = state.diagnostics.get(&filename) {
                    for diag in diag_list {
                        if viewport.overlaps(diag.range) {
                            virtual_texts.push(VirtualText {
                                line: diag.range.start.line,
                                text: diag.message.replace("\n", "  ").clone(),
                                hl_group: state
                                    .diagnosticsDisplay
                                    .get(
                                        &(diag.severity.unwrap_or(DiagnosticSeverity::Hint) as u64),
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
                self.vim()?.command("undo")?;
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
        let mut tokens: Vec<_> = location.split_terminator(':').collect();
        tokens.reverse();
        let filename: String = if tokens.len() > 2 {
            let relpath = tokens
                .pop()
                .ok_or_else(|| format_err!("Failed to get file path! tokens: {:?}", tokens))?
                .to_owned();
            let cwd: String = self.vim()?.eval("getcwd()")?;
            Path::new(&cwd).join(relpath).to_string_lossy().into_owned()
        } else {
            self.vim()?.get_filename(&params)?
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

        self.vim()?.edit(&None, &filename)?;
        self.vim()?.cursor(line + 1, character + 1)?;

        info!("End {}", NOTIFICATION__FZFSinkLocation);
        Ok(())
    }

    pub fn languageClient_FZFSinkCommand(&self, params: &Value) -> Fallible<()> {
        info!("Begin {}", NOTIFICATION__FZFSinkCommand);
        let selection: String =
            try_get("selection", params)?.ok_or_else(|| err_msg("selection not found!"))?;
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

        let params = json!({
            "command": entry.command,
            "arguments": entry.arguments,
        });
        self.workspace_executeCommand(&params)?;

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
        self.preview(diag.message.as_str())?;

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
        self.get_client(&Some(languageId.clone()))?
            .notify(lsp::notification::DidChangeWatchedFiles::METHOD, params)?;

        info!("End {}", lsp::notification::DidChangeWatchedFiles::METHOD);
        Ok(())
    }

    pub fn java_classFileContents(&self, params: &Value) -> Fallible<Value> {
        info!("Begin {}", REQUEST__ClassFileContents);
        let filename = self.vim()?.get_filename(params)?;
        let languageId = self.vim()?.get_languageId(&filename, params)?;

        let content: String = self
            .get_client(&Some(languageId.clone()))?
            .call(REQUEST__ClassFileContents, params)?;

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
