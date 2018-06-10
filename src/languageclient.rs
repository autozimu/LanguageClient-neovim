use super::*;
use lsp::notification::Notification;
use lsp::request::Request;

impl State {
    /////// Utils ///////

    pub fn gather_args<E: VimExp + std::fmt::Debug, T: DeserializeOwned>(
        &mut self,
        exps: &[E],
        map: &Option<Params>,
    ) -> Result<T> {
        let mut map = match *map {
            None | Some(Params::None) | Some(Params::Array(_)) => serde_json::map::Map::new(),
            Some(Params::Map(ref map)) => map.clone(),
        };
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
            result.push(map.remove(&k)
                .ok_or_else(|| format_err!("Failed to get value! k: {}", k))?);
        }

        info!("gather_args: {:?} = {:?}", exps, result);
        Ok(serde_json::from_value(Value::Array(result))?)
    }

    fn sync_settings(&mut self) -> Result<()> {
        let loggingLevel: String = self.eval("get(g:, 'LanguageClient_loggingLevel', 'WARN')")?;
        logger::set_logging_level(&self.logger, &loggingLevel)?;

        #[allow(unknown_lints)]
        #[allow(type_complexity)]
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
        ) = self.eval(
            [
                "!!get(g:, 'LanguageClient_autoStart', 1)",
                "get(g:, 'LanguageClient_serverCommands', {})",
                "get(g:, 'LanguageClient_selectionUI', v:null)",
                "get(g:, 'LanguageClient_trace', v:null)",
                "get(g:, 'LanguageClient_settingsPath', '.vim/settings.json')",
                "!!get(g:, 'LanguageClient_loadSettings', 1)",
                "get(g:, 'LanguageClient_rootMarkers', v:null)",
                "get(g:, 'LanguageClient_changeThrottle', v:null)",
                "get(g:, 'LanguageClient_waitOutputTimeout', v:null)",
                "!!get(g:, 'LanguageClient_diagnosticsEnable', 1)",
                "get(g:, 'LanguageClient_diagnosticsList', 'Quickfix')",
                "get(g:, 'LanguageClient_diagnosticsDisplay', {})",
                "get(g:, 'LanguageClient_windowLogMessageLevel', 'Warning')",
                "get(g:, 'LanguageClient_hoverPreview', 'Auto')",
                "has('nvim')",
            ].as_ref(),
        )?;
        // vimscript use 1 for true, 0 for false.
        let autoStart = autoStart == 1;
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

        let is_nvim = is_nvim == 1;

        self.update(|state| {
            state.autoStart = autoStart;
            state.serverCommands.extend(serverCommands);
            state.selectionUI = selectionUI;
            state.trace = trace;
            state.diagnosticsEnable = diagnosticsEnable;
            state.diagnosticsList = diagnosticsList;
            state.diagnosticsDisplay = serde_json::from_value(
                serde_json::to_value(&state.diagnosticsDisplay)?.combine(diagnosticsDisplay),
            )?;
            state.windowLogMessageLevel = windowLogMessageLevel;
            state.settingsPath = settingsPath;
            state.loadSettings = loadSettings;
            state.rootMarkers = rootMarkers;
            state.change_throttle = change_throttle;
            state.wait_output_timeout = wait_output_timeout;
            state.hoverPreview = hoverPreview;
            state.is_nvim = is_nvim;
            Ok(())
        })?;

        Ok(())
    }

    fn get_workspace_settings(&self, root: &str) -> Result<Value> {
        if !self.loadSettings {
            return Ok(json!({}));
        }

        let buffer = read_to_string(Path::new(root).join(self.settingsPath.clone()))?;
        Ok(serde_json::from_str(&buffer)?)
    }

    fn define_signs(&mut self) -> Result<()> {
        info!("Define signs");
        let cmd = self.get(|state| {
            let mut cmd = "echo".to_owned();

            for entry in state.diagnosticsDisplay.values() {
                cmd += &format!(
                    " | execute 'sign define LanguageClient{} text={} texthl={}'",
                    entry.name, entry.signText, entry.signTexthl,
                );
            }

            Ok(cmd)
        })?;
        self.command(&cmd)?;
        info!("Define signs");
        Ok(())
    }

    fn apply_WorkspaceEdit(&mut self, edit: &WorkspaceEdit, params: &Option<Params>) -> Result<()> {
        debug!(
            "Begin apply WorkspaceEdit: {:?}. Params: {:?}",
            edit, params
        );
        let (filename, line, character): (String, u64, u64) =
            self.gather_args(&[VimVar::Filename, VimVar::Line, VimVar::Character], params)?;

        if let Some(ref changes) = edit.document_changes {
            for e in changes {
                self.apply_TextEdits(&e.text_document.uri.filepath()?, &e.edits)?;
            }
        }
        if let Some(ref changes) = edit.changes {
            for (uri, edits) in changes {
                self.apply_TextEdits(&uri.filepath()?, edits)?;
            }
        }
        self.edit(&None, &filename)?;
        self.jump(line + 1, character + 1)?;
        debug!("End apply WorkspaceEdit");
        Ok(())
    }

    fn apply_TextEdits<P: AsRef<Path>>(&mut self, path: P, edits: &[TextEdit]) -> Result<()> {
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

        self.edit(&None, &path)?;

        let mut lines: Vec<String> = self.call(None, "getline", json!([1, '$']))?;
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
        if self.call::<_, i64>(None, "setline", json!([1, lines]))? != 0 {
            bail!("Failed to set buffer content!");
        }
        debug!("End apply TextEdits");
        Ok(())
    }

    fn update_quickfixlist(&mut self) -> Result<()> {
        let qflist: Vec<_> = self.diagnostics
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

        match self.diagnosticsList {
            DiagnosticsList::Quickfix => {
                self.setqflist(&qflist)?;
            }
            DiagnosticsList::Location => {
                self.setloclist(&qflist)?;
            }
            DiagnosticsList::Disabled => {}
        }

        Ok(())
    }

    fn display_diagnostics(&mut self, filename: &str, diagnostics: &[Diagnostic]) -> Result<()> {
        // Line diagnostics.
        self.update(|state| {
            state
                .line_diagnostics
                .retain(|&(ref f, _), _| f != filename);
            Ok(())
        })?;
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
            state.line_diagnostics.extend(line_diagnostics);
            Ok(())
        })?;

        // Signs.
        if self.text_documents.contains_key(filename) {
            let text = self.text_documents
                .get(filename)
                .ok_or_else(|| format_err!("TextDocumentItem not found! filename: {}", filename))?
                .text
                .clone();
            let lines: Vec<&str> = text.lines().collect();
            let mut signs: Vec<_> = diagnostics
                .iter()
                .map(|dn| {
                    let line = dn.range.start.line;
                    let text = lines
                        .get(line as usize)
                        .map(|l| l.to_string())
                        .unwrap_or_default();
                    let severity = dn.severity.unwrap_or(DiagnosticSeverity::Information);
                    Sign::new(line + 1, text, severity)
                })
                .collect();
            signs.sort_unstable();

            let cmd = self.update(|state| {
                let signs_prev = state.signs.remove(filename).unwrap_or_default();
                let (signs_next, cmd) = get_command_update_signs(&signs_prev, &signs, filename);
                state.signs.insert(filename.to_string(), signs_next);
                Ok(cmd)
            })?;
            info!("Command to update signs: {}", cmd);
            self.command(&cmd)?;
        }

        // Highlight.
        if !self.get(|state| Ok(state.is_nvim))? {
            return Ok(());
        }

        let mut source: Option<u64> = self.get(|state| Ok(state.highlight_source))?;
        if source.is_none() {
            source = Some(self.call(
                None,
                "nvim_buf_add_highlight",
                json!([0, 0, "Error", 1, 1, 1]),
            )?);
            self.update(|state| {
                state.highlight_source = source;
                Ok(())
            })?;
        }
        let source = source.ok_or_else(|| err_msg("Empty highlight source id"))?;
        let diagnosticsDisplay = self.get(|state| Ok(state.diagnosticsDisplay.clone()))?;

        // TODO: Optimize.
        self.call::<_, Option<u8>>(None, "nvim_buf_clear_highlight", json!([0, source, 1, -1]))?;
        for dn in diagnostics {
            let severity = dn.severity.unwrap_or(DiagnosticSeverity::Information);
            let hl_group = diagnosticsDisplay
                .get(&severity.to_int()?)
                .ok_or_else(|| err_msg("Failed to get display"))?
                .texthl
                .clone();
            self.call::<_, u8>(
                None,
                "nvim_buf_add_highlight",
                json!([
                    0,
                    source,
                    hl_group,
                    dn.range.start.line,
                    dn.range.start.character,
                    dn.range.end.character,
                ]),
            )?;
        }

        Ok(())
    }

    fn display_locations(&mut self, locations: &[Location]) -> Result<()> {
        let location_to_quickfix_entry =
            |state: &mut Self, loc: &Location| -> Result<QuickfixEntry> {
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

        match self.get(|state| Ok(state.selectionUI.clone()))? {
            SelectionUI::FZF => {
                let cwd: String = self.eval("getcwd()")?;
                let source: Result<Vec<_>> = locations
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

                self.call::<_, u8>(
                    None,
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Result<Vec<_>> = locations
                    .iter()
                    .map(|loc| location_to_quickfix_entry(self, loc))
                    .collect();
                let list = list?;
                self.setqflist(&list)?;
                self.echo("Quickfix list updated.")?;
            }
            SelectionUI::LocationList => {
                let list: Result<Vec<_>> = locations
                    .iter()
                    .map(|loc| location_to_quickfix_entry(self, loc))
                    .collect();
                let list = list?;
                self.setloclist(&list)?;
                self.echo("Location list updated.")?;
            }
        }
        Ok(())
    }

    fn registerCMSource(&mut self, languageId: &str, result: &Value) -> Result<()> {
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
                let strings: Vec<_> = opt.trigger_characters
                    .unwrap_or_default()
                    .iter()
                    .map(|c| regex::escape(c))
                    .collect();
                strings
            })
            .unwrap_or_default();

        self.call::<_, u8>(
            None,
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

    fn get_line<P: AsRef<Path>>(&mut self, path: P, line: u64) -> Result<String> {
        let value = self.call(
            None,
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

    fn try_handle_command_by_client(&mut self, cmd: &Command) -> Result<bool> {
        if !CommandsClient.contains(&cmd.command.as_str()) {
            return Ok(false);
        }

        if cmd.command == "java.apply.workspaceEdit" {
            if let Some(ref edits) = cmd.arguments {
                for edit in edits {
                    let edit: WorkspaceEdit = serde_json::from_value(edit.clone())?;
                    self.apply_WorkspaceEdit(&edit, &None)?;
                }
            }
        } else {
            bail!("Not implemented: {}", cmd.command);
        }

        Ok(true)
    }

    fn cleanup(&mut self, languageId: &str) -> Result<()> {
        info!("Begin cleanup");

        let root = self.roots
            .get(languageId)
            .cloned()
            .ok_or_else(|| format_err!("No project root found! languageId: {}", languageId))?;

        let mut filenames = vec![];
        for f in self.diagnostics.keys() {
            if f.starts_with(&root) {
                filenames.push(f.clone());
            }
        }
        for f in filenames {
            self.display_diagnostics(&f, &[])?;
        }

        self.diagnostics.retain(|f, _| !f.starts_with(&root));
        self.update_quickfixlist()?;

        self.writers.remove(languageId);
        self.child_ids.remove(languageId);
        self.last_cursor_line = 0;
        self.text_documents.retain(|f, _| !f.starts_with(&root));
        self.roots.remove(languageId);

        self.call::<_, u8>(None, "s:ExecuteAutocmd", "LanguageClientStopped")?;
        self.command(&format!("let {}=0", VIM__ServerStatus))?;
        self.command(&format!("let {}=''", VIM__ServerStatusMessage))?;

        info!("End cleanup");
        Ok(())
    }

    fn preview<S>(&mut self, lines: &[S]) -> Result<()>
    where
        S: AsRef<str> + Serialize,
    {
        let bufname = "//LanguageClient";

        let mut cmd = String::new();
        cmd += "silent! pedit! +setlocal\\ buftype=nofile\\ filetype=markdown\\ nobuflisted\\ noswapfile\\ nonumber ";
        cmd += bufname;
        self.command(cmd)?;

        if self.get(|state| Ok(state.is_nvim))? {
            let bufnr: u64 = serde_json::from_value(self.call(None, "bufnr", bufname)?)?;
            self.call::<_, Option<u8>>(
                None,
                "nvim_buf_set_lines",
                json!([bufnr, 0, -1, 0, lines]),
            )?;
        } else if self.call::<_, i64>(None, "setbufline", json!([bufname, 1, lines]))? != 0 {
            bail!("Failed to set preview buffer content!");
            // TODO: removing existing bottom lines.
        }

        Ok(())
    }

    /////// LSP ///////

    fn initialize(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::Initialize::METHOD);
        let (languageId, filename): (String, String) =
            self.gather_args(&[VimVar::LanguageId, VimVar::Filename], params)?;
        let (rootPath, has_snippet_support): (Option<String>, u64) = self.gather_args(
            &[
                ("rootPath", "v:null"),
                ("hasSnippetSupport", "s:hasSnippetSupport()"),
            ],
            params,
        )?;
        let root = if let Some(r) = rootPath {
            r
        } else {
            let rootMarkers = self.get(|state| Ok(state.rootMarkers.clone()))?;
            let root = get_rootPath(Path::new(&filename), &languageId, &rootMarkers)?
                .to_string_lossy()
                .into_owned();
            self.echomsg(format!("LanguageClient project root: {}", root))?;
            root
        };
        info!("Project root: {}", root);
        let has_snippet_support = has_snippet_support > 0;
        self.update(|state| Ok(state.roots.insert(languageId.clone(), root.clone())))?;

        let initialization_options = self.get_workspace_settings(&root)
            .map(|s| s["initializationOptions"].clone())
            .unwrap_or_else(|err| {
                warn!("Failed to get initializationOptions: {}", err);
                json!({})
            });
        let initialization_options =
            get_default_initializationOptions(&languageId).combine(initialization_options);

        let trace = self.get(|state| Ok(state.trace.clone()))?;

        let result: Value = self.call(
            Some(&languageId),
            lsp::request::Initialize::METHOD,
            InitializeParams {
                process_id: Some(unsafe { libc::getpid() } as u64),
                root_path: Some(root.clone()),
                root_uri: Some(root.to_url()?),
                initialization_options: Some(initialization_options),
                capabilities: ClientCapabilities {
                    text_document: Some(TextDocumentClientCapabilities {
                        completion: Some(CompletionCapability {
                            completion_item: Some(CompletionItemCapability {
                                snippet_support: Some(has_snippet_support),
                                ..CompletionItemCapability::default()
                            }),
                            ..CompletionCapability::default()
                        }),
                        ..TextDocumentClientCapabilities::default()
                    }),
                    ..ClientCapabilities::default()
                },
                trace,
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
            self.echoerr(message)?;
        }

        Ok(result)
    }

    fn initialized(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::Initialized::METHOD);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        self.notify(
            Some(&languageId),
            lsp::notification::Initialized::METHOD,
            InitializedParams {},
        )?;
        info!("End {}", lsp::notification::Initialized::METHOD);
        Ok(())
    }

    pub fn textDocument_hover(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::HoverRequest::METHOD);
        let (languageId, filename, line, character, handle): (
            String,
            String,
            u64,
            u64,
            bool,
        ) = self.gather_args(
            &[
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
                VimVar::Handle,
            ],
            params,
        )?;

        let result = self.call(
            Some(&languageId),
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
            let use_preview = match &self.hoverPreview {
                HoverPreviewOption::Always => true,
                HoverPreviewOption::Never => false,
                HoverPreviewOption::Auto => hover.lines_len() > 1,
            };
            if use_preview {
                self.preview(&hover.to_display())?
            } else {
                self.echo_ellipsis(hover.to_string())?
            }
        }

        info!("End {}", lsp::request::HoverRequest::METHOD);
        Ok(result)
    }

    /// Generic find locations, e.g, definitions, references.
    pub fn find_locations(&mut self, method_name: &str, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", method_name);
        let (buftype, languageId, filename, line, character, goto_cmd, handle): (
            String,
            String,
            String,
            u64,
            u64,
            Option<String>,
            bool,
        ) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
                VimVar::GotoCmd,
                VimVar::Handle,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(&languageId),
            method_name,
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

        let response: Option<GotoDefinitionResponse> = result.clone().to_lsp()?;

        match response {
            None => {
                self.echowarn("Not found!")?;
                return Ok(Value::Null);
            }
            Some(GotoDefinitionResponse::Scalar(loc)) => {
                self.edit(&goto_cmd, loc.uri.filepath()?)?;
                self.jump(loc.range.start.line + 1, loc.range.start.character + 1)?;
            }
            Some(GotoDefinitionResponse::Array(arr)) => match arr.len() {
                0 => self.echowarn("Not found!")?,
                1 => {
                    let loc = arr.get(0).ok_or_else(|| err_msg("Not found!"))?;
                    self.edit(&goto_cmd, loc.uri.filepath()?)?;
                    self.jump(loc.range.start.line + 1, loc.range.start.character + 1)?;
                }
                _ => self.display_locations(&arr)?,
            },
        };

        info!("End {}", method_name);
        Ok(result)
    }

    pub fn textDocument_rename(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::Rename::METHOD);
        let (buftype, languageId, filename, line, character, cword, new_name, handle): (
            String,
            String,
            String,
            u64,
            u64,
            String,
            Option<String>,
            bool,
        ) = self.gather_args(
            &[
                VimVar::Buftype,
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
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let mut new_name = new_name.unwrap_or_default();
        if new_name.is_empty() {
            let value = self.call(None, "s:getInput", ["Rename to: ".to_owned(), cword])?;
            new_name = serde_json::from_value(value)?;
        }
        if new_name.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(&languageId),
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

    pub fn textDocument_documentSymbol(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::DocumentSymbol::METHOD);

        let (buftype, languageId, filename, handle): (String, String, String, bool) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Handle,
            ],
            params,
        )?;

        if !buftype.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(&languageId),
            lsp::request::DocumentSymbol::METHOD,
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

        match self.get(|state| Ok(state.selectionUI.clone()))? {
            SelectionUI::FZF => {
                let source: Vec<_> = symbols
                    .iter()
                    .map(|sym| {
                        let start = sym.location.range.start;
                        format!("{}:{}:\t{}", start.line + 1, start.character + 1, sym.name)
                    })
                    .collect();

                self.call::<_, u8>(
                    None,
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Result<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.setqflist(&list)?;
                self.echo("Document symbols populated to quickfix list.")?;
            }
            SelectionUI::LocationList => {
                let list: Result<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.setloclist(&list)?;
                self.echo("Document symbols populated to location list.")?;
            }
        }

        info!("End {}", lsp::request::DocumentSymbol::METHOD);
        Ok(result)
    }

    pub fn textDocument_codeAction(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::CodeActionRequest::METHOD);
        let (buftype, languageId, filename, line, character, handle): (
            String,
            String,
            String,
            u64,
            u64,
            bool,
        ) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
                VimVar::Handle,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        // Unify filename.
        let filename = filename.canonicalize();

        let diagnostics: Vec<_> = self.diagnostics
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
            .collect();
        let result: Value = self.call(
            Some(&languageId),
            lsp::request::CodeActionRequest::METHOD,
            CodeActionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                range: Range {
                    start: Position { line, character },
                    end: Position { line, character },
                },
                context: CodeActionContext { diagnostics },
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

        self.call::<_, u8>(None, "s:FZF", json!([source, NOTIFICATION__FZFSinkCommand]))?;

        info!("End {}", lsp::request::CodeActionRequest::METHOD);
        Ok(result)
    }

    pub fn textDocument_completion(&mut self, params: &Option<Params>) -> Result<Value> {
        // Vim will change buffer content temporarily when executing omnifunc (?).
        // self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::Completion::METHOD);

        let (buftype, languageId, filename, line, character, handle): (
            String,
            String,
            String,
            u64,
            u64,
            bool,
        ) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
                VimVar::Handle,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(&languageId),
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

    pub fn textDocument_signatureHelp(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::SignatureHelpRequest::METHOD);
        let (buftype, languageId, filename, line, character, handle): (
            String,
            String,
            String,
            u64,
            u64,
            bool,
        ) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
                VimVar::Handle,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(&languageId),
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
        let active_signature = help.signatures
            .get(help.active_signature.unwrap_or(0).to_usize()?)
            .ok_or_else(|| err_msg("Failed to get active signature"))?;
        let active_parameter: Option<&ParameterInformation>;
        if let Some(ref parameters) = active_signature.parameters {
            active_parameter = parameters.get(help.active_parameter.unwrap_or(0).to_usize()?);
        } else {
            active_parameter = None;
        }

        if let Some(active_parameter) = active_parameter {
            let mut cmd = "echo".to_owned();
            let chunks: Vec<&str> = active_signature
                .label
                .split(&active_parameter.label)
                .collect();
            if chunks.len() == 2 {
                let begin = chunks.get(0).cloned().unwrap_or_default();
                let end = chunks.get(1).cloned().unwrap_or_default();
                cmd += &format!(
                    " | echon '{}' | echohl WarningMsg | echon '{}' | echohl None | echon '{}'",
                    begin, active_parameter.label, end
                );
            } else {
                // Active parameter is not part of signature.
                cmd += &format!(" | echo '{}'", active_signature.label);
            }
            self.command(&cmd)?;
        } else {
            self.echo(&active_signature.label)?;
        }

        info!("End {}", lsp::request::SignatureHelpRequest::METHOD);
        Ok(Value::Null)
    }

    pub fn textDocument_references(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::References::METHOD);

        let (buftype, languageId, filename, line, character, handle, include_declaration): (
            String,
            String,
            String,
            u64,
            u64,
            bool,
            bool,
        ) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
                VimVar::Handle,
                VimVar::IncludeDeclaration,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(&languageId),
            lsp::request::References::METHOD,
            ReferenceParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position: Position { line, character },
                context: ReferenceContext {
                    include_declaration,
                },
            },
        )?;

        if !handle {
            return Ok(result);
        }

        let locations: Vec<Location> = serde_json::from_value(result.clone())?;
        self.display_locations(&locations)?;

        info!("End {}", lsp::request::References::METHOD);
        Ok(result)
    }

    pub fn textDocument_formatting(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::Formatting::METHOD);
        let (buftype, languageId, filename, handle): (String, String, String, bool) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Handle,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let (tab_size, insert_spaces): (u64, u64) =
            self.eval(["shiftwidth()", "&expandtab"].as_ref())?;
        let insert_spaces = insert_spaces == 1;
        let result = self.call(
            Some(&languageId),
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
            changes: Some(hashmap!{filename.to_url()? => text_edits}),
            document_changes: None,
        };
        self.apply_WorkspaceEdit(&edit, params)?;
        info!("End {}", lsp::request::Formatting::METHOD);
        Ok(result)
    }

    pub fn textDocument_rangeFormatting(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::RangeFormatting::METHOD);
        let (buftype, languageId, filename, handle, tab_size, insert_spaces, start_line, end_line):
            (String, String, String, bool, u64, u64, u64, u64) = self.gather_args(
                &[
                VimVar::Buftype.to_key().as_str(),
                VimVar::LanguageId.to_key().as_str(),
                VimVar::Filename.to_key().as_str(),
                VimVar::Handle.to_key().as_str(),
                "&tabstop",
                "&expandtab",
                "LSP#range_start_line()",
                "LSP#range_end_line()",
                ], params)?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let insert_spaces = insert_spaces == 1;
        let result = self.call(
            Some(&languageId),
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
            changes: Some(hashmap!{filename.to_url()? => text_edits}),
            document_changes: None,
        };
        self.apply_WorkspaceEdit(&edit, params)?;
        info!("End {}", lsp::request::RangeFormatting::METHOD);
        Ok(result)
    }

    pub fn completionItem_resolve(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::ResolveCompletionItem::METHOD);
        let (buftype, languageId, handle): (String, String, bool) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Handle],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }
        let (completion_item,): (CompletionItem,) = self.gather_args(&["completionItem"], params)?;

        let result = self.call(
            Some(&languageId),
            lsp::request::ResolveCompletionItem::METHOD,
            completion_item,
        )?;

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

    pub fn workspace_symbol(&mut self, params: &Option<Params>) -> Result<Value> {
        self.textDocument_didChange(params)?;
        info!("Begin {}", lsp::request::WorkspaceSymbol::METHOD);
        let (buftype, languageId, handle): (String, String, bool) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Handle],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let (query,): (String,) = self.gather_args(&[("query", "")], params)?;
        let result = self.call(
            Some(&languageId),
            lsp::request::WorkspaceSymbol::METHOD,
            WorkspaceSymbolParams { query },
        )?;

        if !handle {
            return Ok(result);
        }

        let symbols: Vec<SymbolInformation> = serde_json::from_value(result.clone())?;

        match self.get(|state| Ok(state.selectionUI.clone()))? {
            SelectionUI::FZF => {
                let cwd: String = self.eval("getcwd()")?;
                let source: Result<Vec<_>> = symbols
                    .iter()
                    .map(|sym| {
                        let filename = sym.location.uri.filepath()?;
                        let relpath = diff_paths(&filename, Path::new(&cwd)).unwrap_or(filename);
                        let start = sym.location.range.start;
                        Ok(format!(
                            "{}:{}:{}:\t{}",
                            relpath.to_string_lossy(),
                            start.line + 1,
                            start.character + 1,
                            sym.name
                        ))
                    })
                    .collect();
                let source = source?;

                self.call::<_, u8>(
                    None,
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::Quickfix => {
                let list: Result<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.setqflist(&list)?;
                self.echo("Workspace symbols populated to quickfix list.")?;
            }
            SelectionUI::LocationList => {
                let list: Result<Vec<_>> = symbols.iter().map(QuickfixEntry::from_lsp).collect();
                let list = list?;
                self.setloclist(&list)?;
                self.echo("Workspace symbols populated to location list.")?;
            }
        }

        info!("End {}", lsp::request::WorkspaceSymbol::METHOD);
        Ok(result)
    }

    pub fn workspace_executeCommand(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::ExecuteCommand::METHOD);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let (command, arguments): (String, Vec<Value>) =
            self.gather_args(&["command", "arguments"], params)?;

        let result = self.call(
            Some(&languageId),
            lsp::request::ExecuteCommand::METHOD,
            ExecuteCommandParams { command, arguments },
        )?;
        info!("End {}", lsp::request::ExecuteCommand::METHOD);
        Ok(result)
    }

    pub fn workspace_applyEdit(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::ApplyWorkspaceEdit::METHOD);

        let params: ApplyWorkspaceEditParams = params.clone().to_lsp()?;
        self.apply_WorkspaceEdit(&params.edit, &None)?;

        info!("End {}", lsp::request::ApplyWorkspaceEdit::METHOD);

        Ok(serde_json::to_value(ApplyWorkspaceEditResponse {
            applied: true,
        })?)
    }

    pub fn workspace_didChangeConfiguration(&mut self, params: &Option<Params>) -> Result<()> {
        info!(
            "Begin {}",
            lsp::notification::DidChangeConfiguration::METHOD
        );
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let (settings,): (Value,) = self.gather_args(&["settings"], params)?;

        self.notify(
            Some(languageId.as_str()),
            lsp::notification::DidChangeConfiguration::METHOD,
            DidChangeConfigurationParams { settings },
        )?;
        info!("End {}", lsp::notification::DidChangeConfiguration::METHOD);
        Ok(())
    }

    pub fn textDocument_didOpen(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::DidOpenTextDocument::METHOD);
        let (buftype, languageId, filename, text): (String, String, String, Vec<String>) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Text,
            ],
            params,
        )?;

        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }

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

        self.notify(
            Some(&languageId),
            lsp::notification::DidOpenTextDocument::METHOD,
            DidOpenTextDocumentParams { text_document },
        )?;

        self.command("setlocal omnifunc=LanguageClient#complete")?;
        if self.get(|state| Ok(state.text_documents.contains_key(&filename)))? {
            self.call::<_, u8>(None, "s:ExecuteAutocmd", "LanguageClientBufReadPost")?;
        }

        info!("End {}", lsp::notification::DidOpenTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_didChange(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::DidChangeTextDocument::METHOD);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Filename],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }
        if !self.get(|state| Ok(state.text_documents.contains_key(&filename)))? {
            info!("Not opened yet. Switching to didOpen.");
            return self.textDocument_didOpen(params);
        }

        let (text,): (Vec<String>,) = self.gather_args(&[VimVar::Text], params)?;

        let text = text.join("\n");
        let text_state = self.get(|state| {
            state
                .text_documents
                .get(&filename)
                .ok_or_else(|| format_err!("TextDocumentItem not found! filename: {}", filename))
                .map(|doc| doc.text.clone())
        }).unwrap_or_default();
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

        self.notify(
            Some(&languageId),
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

    pub fn textDocument_didSave(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::DidSaveTextDocument::METHOD);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Filename],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }
        let uri = filename.to_url()?;

        self.notify(
            Some(&languageId),
            lsp::notification::DidSaveTextDocument::METHOD,
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
            },
        )?;

        info!("End {}", lsp::notification::DidSaveTextDocument::METHOD);
        Ok(())
    }

    pub fn textDocument_didClose(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::DidCloseTextDocument::METHOD);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Filename],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }

        self.notify(
            Some(&languageId),
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

    pub fn textDocument_publishDiagnostics(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::PublishDiagnostics::METHOD);
        let params: PublishDiagnosticsParams = params.clone().to_lsp()?;
        if !self.get(|state| Ok(state.diagnosticsEnable))? {
            return Ok(());
        }

        let mut filename = params.uri.filepath()?.to_string_lossy().into_owned();
        // Workaround bug: remove first '/' in case of '/C:/blabla'.
        if filename.chars().nth(0) == Some('/') && filename.chars().nth(2) == Some(':') {
            filename.remove(0);
        }
        // Unify name to avoid mismatch due to case insensitivity.
        let filename = filename.canonicalize();

        self.update(|state| {
            state
                .diagnostics
                .insert(filename.clone(), params.diagnostics.clone());
            Ok(())
        })?;
        self.update_quickfixlist()?;

        let current_filename: String = self.eval(VimVar::Filename)?;
        if filename != current_filename.canonicalize() {
            return Ok(());
        }
        self.display_diagnostics(&current_filename, &params.diagnostics)?;
        self.call::<_, u8>(None, "s:ExecuteAutocmd", "LanguageClientDiagnosticsChanged")?;

        info!("End {}", lsp::notification::PublishDiagnostics::METHOD);
        Ok(())
    }

    pub fn window_logMessage(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::LogMessage::METHOD);
        let params: LogMessageParams = params.clone().to_lsp()?;
        let threshold = self.get(|state| state.windowLogMessageLevel.to_int())?;
        if params.typ.to_int()? > threshold {
            return Ok(());
        }

        let msg = format!("[{:?}] {}", params.typ, params.message);
        self.echomsg(&msg)?;
        info!("End {}", lsp::notification::LogMessage::METHOD);
        Ok(())
    }

    pub fn window_showMessage(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::ShowMessage::METHOD);
        let params: ShowMessageParams = params.clone().to_lsp()?;
        let msg = format!("[{:?}] {}", params.typ, params.message);
        self.echomsg(&msg)?;
        info!("End {}", lsp::notification::ShowMessage::METHOD);
        Ok(())
    }

    pub fn client_registerCapability(
        &mut self,
        _languageId: &str,
        params: &Option<Params>,
    ) -> Result<Value> {
        info!("Begin {}", lsp::request::RegisterCapability::METHOD);
        let params: RegistrationParams = params.clone().to_lsp()?;
        for r in &params.registrations {
            match r.method.as_str() {
                lsp::notification::DidChangeWatchedFiles::METHOD => {
                    let opt: DidChangeWatchedFilesRegistrationOptions =
                        serde_json::from_value(r.register_options.clone().unwrap_or_default())?;
                    if let Some(ref mut watcher) = self.watcher {
                        for w in opt.watchers {
                            watcher.watch(w.glob_pattern, notify::RecursiveMode::NonRecursive)?;
                        }
                    }
                }
                _ => {
                    warn!("Unknown registration: {:?}", r);
                }
            }
        }

        self.registrations.extend(params.registrations);
        info!("End {}", lsp::request::RegisterCapability::METHOD);
        Ok(Value::Null)
    }

    pub fn client_unregisterCapability(
        &mut self,
        _languageId: &str,
        params: &Option<Params>,
    ) -> Result<Value> {
        info!("Begin {}", lsp::request::UnregisterCapability::METHOD);
        let params: UnregistrationParams = params.clone().to_lsp()?;
        let mut regs_removed = vec![];
        for r in &params.unregisterations {
            if let Some(idx) = self.registrations
                .iter()
                .position(|i| i.id == r.id && i.method == r.method)
            {
                regs_removed.push(self.registrations.swap_remove(idx));
            }
        }

        for r in &regs_removed {
            match r.method.as_str() {
                lsp::notification::DidChangeWatchedFiles::METHOD => {
                    let opt: DidChangeWatchedFilesRegistrationOptions =
                        serde_json::from_value(r.register_options.clone().unwrap_or_default())?;
                    if let Some(ref mut watcher) = self.watcher {
                        for w in opt.watchers {
                            watcher.unwatch(w.glob_pattern)?;
                        }
                    }
                }
                _ => {
                    warn!("Unknown registration: {:?}", r);
                }
            }
        }

        info!("End {}", lsp::request::UnregisterCapability::METHOD);
        Ok(Value::Null)
    }

    pub fn exit(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::Exit::METHOD);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;

        let result = self.notify(
            Some(&languageId),
            lsp::notification::Exit::METHOD,
            Value::Null,
        );
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

    pub fn languageClient_getState(&mut self, _params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__GetState);
        let s = self.get(|state| Ok(serde_json::to_string(state)?))?;
        info!("End {}", REQUEST__GetState);
        Ok(Value::String(s))
    }

    pub fn languageClient_isAlive(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__IsAlive);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let is_alive = self.get(|state| Ok(state.writers.contains_key(&languageId)))?;
        info!("End {}", REQUEST__IsAlive);
        Ok(Value::Bool(is_alive))
    }

    pub fn languageClient_registerServerCommands(
        &mut self,
        params: &Option<Params>,
    ) -> Result<Value> {
        info!("Begin {}", REQUEST__RegisterServerCommands);
        let commands: HashMap<String, Vec<String>> = params.clone().to_lsp()?;
        self.update(|state| {
            state.serverCommands.extend(commands);
            Ok(())
        })?;
        let exp = format!(
            "let g:LanguageClient_serverCommands={}",
            serde_json::to_string(&self.get(|state| Ok(state.serverCommands.clone()))?)?
        );
        self.command(&exp)?;
        info!("End {}", REQUEST__RegisterServerCommands);
        Ok(Value::Null)
    }

    pub fn languageClient_setLoggingLevel(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__SetLoggingLevel);
        let (loggingLevel,): (String,) = self.gather_args(&["loggingLevel"], params)?;
        logger::set_logging_level(&self.logger, &loggingLevel)?;
        info!("End {}", REQUEST__SetLoggingLevel);
        Ok(Value::Null)
    }

    pub fn languageClient_registerHandlers(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__RegisterHandlers);
        let handlers: HashMap<String, String> = params.clone().to_lsp()?;
        self.update(|state| {
            state.user_handlers.extend(handlers);
            Ok(())
        })?;
        info!("End {}", REQUEST__RegisterHandlers);
        Ok(Value::Null)
    }

    pub fn languageClient_omniComplete(&mut self, params: &Option<Params>) -> Result<Value> {
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

        let matches: Result<Vec<VimCompleteItem>> = matches.iter().map(FromLSP::from_lsp).collect();
        let matches = matches?;
        info!("End {}", REQUEST__OmniComplete);
        Ok(serde_json::to_value(matches)?)
    }

    pub fn languageClient_handleBufReadPost(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__HandleBufReadPost);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Filename],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() || filename.is_empty() {
            return Ok(());
        }

        // File opened before.
        if self.get(|state| Ok(state.text_documents.contains_key(&filename)))? {
            info!("File is opened before.");
            return Ok(());
        }

        if self.get(|state| Ok(state.writers.contains_key(&languageId)))? {
            self.textDocument_didOpen(params)?;

            let diagnostics = self.get(|state| {
                state
                    .diagnostics
                    .get(&filename.canonicalize())
                    .cloned()
                    .ok_or_else(|| format_err!("No diagnostics! filename: {}", filename))
            }).unwrap_or_default();
            self.display_diagnostics(&filename, &diagnostics)?;
            self.languageClient_handleCursorMoved(params)?;
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

        info!("End {}", NOTIFICATION__HandleBufReadPost);
        Ok(())
    }

    pub fn languageClient_handleTextChanged(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__HandleTextChanged);
        let (buftype, filename): (String, String) =
            self.gather_args(&[VimVar::Buftype, VimVar::Filename], params)?;
        if !buftype.is_empty() {
            info!(
                "Skip handleTextChanged as buftype is non-empty: {}",
                buftype
            );
        }
        let skip_notification = self.get(|state| {
            if let Some(metadata) = state.text_documents_metadata.get(&filename) {
                if let Some(throttle) = state.change_throttle {
                    if metadata.last_change.elapsed() < throttle {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        })?;
        if skip_notification {
            info!("Skip handleTextChanged due to throttling");
            return Ok(());
        }

        self.textDocument_didChange(params)?;
        info!("End {}", NOTIFICATION__HandleTextChanged);
        Ok(())
    }

    pub fn languageClient_handleBufWritePost(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__HandleBufWritePost);
        self.textDocument_didSave(params)?;
        info!("End {}", NOTIFICATION__HandleBufWritePost);
        Ok(())
    }

    pub fn languageClient_handleBufDelete(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__HandleBufWritePost);
        let (filename,): (String,) = self.gather_args(&[VimVar::Filename], params)?;
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

    pub fn languageClient_handleCursorMoved(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__HandleCursorMoved);
        let (buftype, filename, line): (String, String, u64) =
            self.gather_args(&[VimVar::Buftype, VimVar::Filename, VimVar::Line], params)?;
        if !buftype.is_empty() || line == self.get(|state| Ok(state.last_cursor_line))? {
            return Ok(());
        }

        self.update(|state| {
            state.last_cursor_line = line;
            Ok(())
        })?;
        let message = self.get(|state| {
            state
                .line_diagnostics
                .get(&(filename.clone(), line))
                .cloned()
                .ok_or_else(|| {
                    format_err!(
                        "Line diagnostic message not found! filename: {}, line: {}",
                        filename,
                        line
                    )
                })
        }).unwrap_or_default();
        if message == self.get(|state| Ok(state.last_line_diagnostic.clone()))? {
            return Ok(());
        }

        self.update(|state| {
            state.last_line_diagnostic = message.clone();
            Ok(())
        })?;
        self.echo_ellipsis(&message)?;

        info!("End {}", NOTIFICATION__HandleCursorMoved);
        Ok(())
    }

    pub fn languageClient_handleCompleteDone(&mut self, params: &Option<Params>) -> Result<()> {
        let (filename, completed_item): (String, VimCompleteItem) = self.gather_args(
            &[VimVar::Filename.to_key().as_str(), "completed_item"],
            params,
        )?;
        let user_data = match completed_item.user_data {
            Some(data) => data,
            None => return Ok(()),
        };
        let user_data: VimCompleteItemUserData = serde_json::from_str(&user_data)?;

        let mut edits = vec![];
        if let Some(edit) = user_data.text_edit {
            edits.push(edit.clone());
        };
        if let Some(aedits) = user_data.additional_text_edits {
            edits.extend(aedits.clone());
        };

        // TODO
        // 1. undo previous completion
        // 2. relocate cursor
        self.apply_TextEdits(filename, &edits)
    }

    pub fn languageClient_FZFSinkLocation(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__FZFSinkLocation);
        let params = match *params {
            None | Some(Params::None) | Some(Params::Map(_)) => {
                bail!("Expecting array params!");
            }
            Some(Params::Array(ref arr)) => Value::Array(arr.clone()),
        };

        let lines: Vec<String> = serde_json::from_value(params)?;
        if lines.is_empty() {
            err_msg("No selection!");
        }
        let mut tokens: Vec<&str> = lines
            .get(0)
            .ok_or_else(|| format_err!("Failed to get line! lines: {:?}", lines))?
            .split(':')
            .collect();
        tokens.reverse();
        let filename: String = if tokens.len() > 3 {
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
            .to_int()? - 1;
        let character = tokens
            .pop()
            .ok_or_else(|| format_err!("Failed to get character! tokens: {:?}", tokens))?
            .to_int()? - 1;

        self.edit(&None, &filename)?;
        self.jump(line + 1, character + 1)?;

        info!("End {}", NOTIFICATION__FZFSinkLocation);
        Ok(())
    }

    pub fn languageClient_FZFSinkCommand(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__FZFSinkCommand);
        let (selection,): (String,) = self.gather_args(&["selection"], params)?;
        let tokens: Vec<&str> = selection.split(": ").collect();
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
        })?;

        if self.try_handle_command_by_client(&entry)? {
            return Ok(());
        }

        self.workspace_executeCommand(&json!({
                "command": entry.command,
                "arguments": entry.arguments,
            }).to_params()?)?;

        self.update(|state| {
            state.stashed_codeAction_commands = vec![];
            Ok(())
        })?;

        info!("End {}", NOTIFICATION__FZFSinkCommand);
        Ok(())
    }

    pub fn NCM_refresh(&mut self, params: &Option<Params>) -> Result<Value> {
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
                "buftype": "",
                "languageId": ctx.filetype,
                "filename": filename,
                "line": line,
                "character": character,
                "handle": false,
            }).to_params()?)?;
        let result: Option<CompletionResponse> = serde_json::from_value(result)?;
        let result = result.unwrap_or_else(|| CompletionResponse::Array(vec![]));
        let is_incomplete = match result {
            CompletionResponse::Array(_) => false,
            CompletionResponse::List(ref list) => list.is_incomplete,
        };
        let matches: Result<Vec<VimCompleteItem>> = match result {
            CompletionResponse::Array(arr) => arr,
            CompletionResponse::List(list) => list.items,
        }.iter()
            .map(FromLSP::from_lsp)
            .collect();
        let matches = matches?;
        self.call::<_, u8>(
            None,
            "cm#complete",
            json!([info.name, ctx, ctx.startcol, matches, is_incomplete]),
        )?;
        info!("End {}", REQUEST__NCMRefresh);
        Ok(Value::Null)
    }

    pub fn languageClient_explainErrorAtPoint(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__ExplainErrorAtPoint);
        let (buftype, filename, line, character): (String, String, u64, u64) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
            ],
            params,
        )?;
        if !buftype.is_empty() {
            return Ok(Value::Null);
        }
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
        })?;
        let message: Vec<_> = diag.message.lines().collect();
        self.preview(&message)?;

        info!("End {}", REQUEST__ExplainErrorAtPoint);
        Ok(Value::Null)
    }

    // Extensions by languge servers.
    pub fn language_status(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__LanguageStatus);
        let params: LanguageStatusParams = params.clone().to_lsp()?;
        let msg = format!("{} {}", params.typee, params.message);
        self.echomsg(&msg)?;
        info!("End {}", NOTIFICATION__LanguageStatus);
        Ok(())
    }

    pub fn rustDocument_implementations(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__RustImplementations);
        let (buftype, languageId, filename, line, character, handle): (
            String,
            String,
            String,
            u64,
            u64,
            bool,
        ) = self.gather_args(
            &[
                VimVar::Buftype,
                VimVar::LanguageId,
                VimVar::Filename,
                VimVar::Line,
                VimVar::Character,
                VimVar::Handle,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(&languageId),
            REQUEST__RustImplementations,
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

        let locations: Vec<Location> = serde_json::from_value(result.clone())?;
        self.display_locations(&locations)?;

        info!("End {}", REQUEST__RustImplementations);
        Ok(result)
    }

    pub fn rust_handleBeginBuild(&mut self, _params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__RustBeginBuild);
        self.command(&format!(
            "let {}=1 | let {}='Rust: build begin'",
            VIM__ServerStatus, VIM__ServerStatusMessage
        ))?;
        info!("End {}", NOTIFICATION__RustBeginBuild);
        Ok(())
    }

    pub fn rust_handleDiagnosticsBegin(&mut self, _params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__RustDiagnosticsBegin);
        self.command(&format!(
            "let {}=1 | let {}='Rust: diagnostics begin'",
            VIM__ServerStatus, VIM__ServerStatusMessage
        ))?;
        info!("End {}", NOTIFICATION__RustDiagnosticsBegin);
        Ok(())
    }

    pub fn rust_handleDiagnosticsEnd(&mut self, _params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__RustDiagnosticsEnd);
        self.command(&format!(
            "let {}=0 | let {}='Rust: diagnostics end'",
            VIM__ServerStatus, VIM__ServerStatusMessage
        ))?;
        info!("End {}", NOTIFICATION__RustDiagnosticsEnd);
        Ok(())
    }

    pub fn window_progress(&mut self, params: &Option<Params>) -> Result<()> {
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

        self.command(&format!(
            "let {}={} | let {}='{}'",
            VIM__ServerStatus,
            if done { 0 } else { 1 },
            VIM__ServerStatusMessage,
            &escape_single_quote(buf)
        ))?;
        info!("End {}", NOTIFICATION__WindowProgress);
        Ok(())
    }

    pub fn cquery_handleProgress(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__CqueryProgress);
        let params: CqueryProgressParams = params.clone().to_lsp()?;
        let total = params.indexRequestCount + params.doIdMapCount + params.loadPreviousIndexCount
            + params.onIdMappedCount + params.onIndexedCount;
        if total != 0 {
            self.command(&format!(
                "let {}=1 | let {}='cquery: indexing ({} jobs)'",
                VIM__ServerStatus, VIM__ServerStatusMessage, params.indexRequestCount
            ))?;
        } else {
            self.command(&format!(
                "let {}=0 | let {}='cquery: idle'",
                VIM__ServerStatus, VIM__ServerStatusMessage
            ))?;
        }
        info!("End {}", NOTIFICATION__CqueryProgress);
        Ok(())
    }

    pub fn languageClient_startServer(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__StartServer);
        let (cmdargs,): (Vec<String>,) = self.gather_args(&[("cmdargs", "[]")], params)?;
        let cmdparams = vim_cmd_args_to_value(&cmdargs)?;
        let params = rpc::to_value(params.clone())?;
        let params = params.combine(cmdparams).to_params()?;
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Filename],
            &params,
        )?;

        if !buftype.is_empty() || filename.is_empty() {
            return Ok(Value::Null);
        }

        if self.get(|state| Ok(state.writers.contains_key(&languageId)))? {
            bail!(
                "Language client has already started for language {}.",
                &languageId
            );
        }

        self.sync_settings()?;

        let command = self.get(|state| {
            state
                .serverCommands
                .get(&languageId)
                .cloned()
                .ok_or_else(|| {
                    format_err!(
                        "No language server command found for file type: {}.",
                        &languageId
                    )
                })
        })?;

        let (child_id, reader, writer): (_, Box<SyncRead>, Box<SyncWrite>) =
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
                let home = env::home_dir().ok_or_else(|| err_msg("Failed to get home dir"))?;
                let command: Vec<_> = command
                    .into_iter()
                    .map(|cmd| {
                        if cmd.starts_with('~') {
                            cmd.replacen('~', &home.to_string_lossy(), 1)
                        } else {
                            cmd
                        }
                    })
                    .collect();

                let stderr = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&get_logpath_server())?;

                let process = std::process::Command::new(command
                    .get(0)
                    .ok_or_else(|| err_msg("Empty command!"))?)
                    .args(&command[1..])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(stderr)
                    .spawn()?;

                let child_id = Some(process.id());
                let reader = Box::new(BufReader::new(process
                    .stdout
                    .ok_or_else(|| err_msg("Failed to get subprocess stdout"))?));
                let writer = Box::new(BufWriter::new(process
                    .stdin
                    .ok_or_else(|| err_msg("Failed to get subprocess stdin"))?));
                (child_id, reader, writer)
            };

        self.update(|state| {
            child_id.map(|id| state.child_ids.insert(languageId.clone(), id));
            state.writers.insert(languageId.clone(), writer);
            Ok(())
        })?;

        let thread_name = format!("reader-{}", languageId);
        let languageId_clone = languageId.clone();
        let tx = self.tx.clone();
        std::thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || {
                if let Err(err) = vim::loop_reader(reader, &Some(languageId_clone.clone()), &tx) {
                    let _ = tx.send(Message::Notification(
                        Some(languageId_clone.clone()),
                        rpc::Notification {
                            jsonrpc: None,
                            method: NOTIFICATION__ServerExited.into(),
                            params: json!({
                                "languageId": languageId_clone,
                                "message": format!("{}", err),
                            }).to_params()
                                .unwrap_or_default(),
                        },
                    ));
                }
            })?;

        info!("End {}", REQUEST__StartServer);

        if self.get(|state| Ok(state.writers.len()))? == 1 {
            self.define_signs()?;
        }

        self.initialize(&params)?;
        self.initialized(&params)?;

        let root = self.roots.get(&languageId).cloned().unwrap_or_default();
        let settings = self.get_workspace_settings(&root);
        if let Err(ref err) = settings {
            warn!("Failed to get workspace settings: {}", err);
        }
        self.workspace_didChangeConfiguration(&json!({
            VimVar::LanguageId.to_key(): languageId,
            "settings": settings.unwrap_or_else(|_| json!({})),
        }).to_params()?)?;

        self.textDocument_didOpen(&params)?;
        self.textDocument_didChange(&params)?;

        self.call::<_, u8>(None, "s:ExecuteAutocmd", "LanguageClientStarted")?;
        Ok(Value::Null)
    }

    pub fn languageClient_serverExited(&mut self, params: &Option<Params>) -> Result<()> {
        let (languageId, message): (String, String) = self.gather_args(
            [VimVar::LanguageId.to_key().as_str(), "message"].as_ref(),
            params,
        )?;

        if self.writers.contains_key(&languageId) {
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

    pub fn check_fs_notify(&mut self) -> Result<()> {
        if self.watcher.is_some() {
            let mut events = vec![];
            loop {
                let result = self.watcher_rx.try_recv();
                let event = match result {
                    Ok(event) => event,
                    Err(TryRecvError::Empty) => {
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        bail!("File system notification channel disconnected!");
                    }
                };
                events.push(event);
            }

            if events.is_empty() {
                return Ok(());
            }

            let mut changes = vec![];
            for e in events {
                if let Ok(c) = e.to_lsp() {
                    changes.extend(c);
                }
            }

            use DidChangeWatchedFilesParams as P;
            self.workspace_didChangeWatchedFiles(&P { changes }.to_params()?)?;
        }

        Ok(())
    }

    pub fn workspace_didChangeWatchedFiles(&mut self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::DidChangeWatchedFiles::METHOD);
        let (languageId,): (String,) = self.gather_args([VimVar::LanguageId].as_ref(), params)?;

        let params: DidChangeWatchedFilesParams = params.clone().to_lsp()?;
        self.notify(
            Some(&languageId),
            lsp::notification::DidChangeWatchedFiles::METHOD,
            params,
        )?;

        info!("End {}", lsp::notification::DidChangeWatchedFiles::METHOD);
        Ok(())
    }

    pub fn java_classFileContents(&mut self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__ClassFileContents);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;

        let content: String = self.call(
            Some(languageId.as_str()),
            REQUEST__ClassFileContents,
            params,
        )?;

        info!("End {}", REQUEST__ClassFileContents);
        Ok(Value::String(content))
    }
}
