use super::*;
use std::str::FromStr;
use std::ops::Deref;
use lsp::request::Request;
use lsp::notification::Notification;
use vim::*;

pub trait ILanguageClient {
    fn get<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&State) -> Result<T>;
    fn update<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut State) -> Result<T>;
    fn loop_message<T: BufRead>(&self, input: T, languageId: Option<String>) -> Result<()>;
    fn handle_message(&self, languageId: Option<String>, message: String) -> Result<()>;
    fn write(&self, languageId: Option<&str>, message: &str) -> Result<()>;
    fn output(&self, languageId: Option<&str>, id: Id, result: Result<Value>) -> Result<()>;
    fn call<P: Serialize>(&self, languageId: Option<&str>, method: &str, params: P) -> Result<Value>;
    fn notify<P: Serialize>(&self, languageId: Option<&str>, method: &str, params: P) -> Result<()>;

    // Utils.
    fn gather_args<E: VimExp + std::fmt::Debug, T: DeserializeOwned>(
        &self,
        exps: &[E],
        params: &Option<Params>,
    ) -> Result<T>;
    fn sync_settings(&self) -> Result<()>;
    fn define_signs(&self) -> Result<()>;
    fn apply_WorkspaceEdit(&self, edit: &WorkspaceEdit, params: &Option<Params>) -> Result<()>;
    fn apply_TextEdits<P: AsRef<Path>>(&self, path: P, edits: &[TextEdit]) -> Result<()>;
    fn display_diagnostics(&self, filename: &str, diagnostics: &[Diagnostic]) -> Result<()>;
    fn display_locations(&self, locations: &[Location], languageId: &str) -> Result<()>;
    fn registerCMSource(&self, languageId: &str, result: &Value) -> Result<()>;
    fn get_line<P: AsRef<Path>>(&self, path: P, line: u64) -> Result<String>;
    fn try_handle_command_by_client(&self, cmd: &Command) -> Result<bool>;
    fn cleanup(&self, languageId: &str) -> Result<()>;

    fn initialize(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_hover(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_definition(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_rename(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_documentSymbol(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_codeAction(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_completion(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_signatureHelp(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_references(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_formatting(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_rangeFormatting(&self, params: &Option<Params>) -> Result<Value>;
    fn completionItem_resolve(&self, params: &Option<Params>) -> Result<Value>;
    fn workspace_symbol(&self, params: &Option<Params>) -> Result<Value>;
    fn workspace_executeCommand(&self, params: &Option<Params>) -> Result<Value>;
    fn workspace_applyEdit(&self, params: &Option<Params>) -> Result<Value>;
    fn rustDocument_implementations(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_didOpen(&self, params: &Option<Params>) -> Result<()>;
    fn textDocument_didChange(&self, params: &Option<Params>) -> Result<()>;
    fn textDocument_didSave(&self, params: &Option<Params>) -> Result<()>;
    fn textDocument_didClose(&self, params: &Option<Params>) -> Result<()>;
    fn textDocument_publishDiagnostics(&self, params: &Option<Params>) -> Result<()>;
    fn window_logMessage(&self, params: &Option<Params>) -> Result<()>;
    fn exit(&self, params: &Option<Params>) -> Result<()>;

    // Extensions.
    fn languageClient_getState(&self, &Option<Params>) -> Result<Value>;
    fn languageClient_isAlive(&self, &Option<Params>) -> Result<Value>;
    fn languageClient_startServer(&self, params: &Option<Params>) -> Result<Value>;
    fn languageClient_registerServerCommands(&self, params: &Option<Params>) -> Result<Value>;
    fn languageClient_setLoggingLevel(&self, params: &Option<Params>) -> Result<Value>;
    fn languageClient_omniComplete(&self, params: &Option<Params>) -> Result<Value>;
    fn languageClient_handleBufReadPost(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_handleTextChanged(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_handleBufWritePost(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_handleBufDelete(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_handleCursorMoved(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_FZFSinkLocation(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_FZFSinkCommand(&self, params: &Option<Params>) -> Result<()>;
    fn NCM_refresh(&self, params: &Option<Params>) -> Result<()>;

    // Extensions by languge servers.
    fn language_status(&self, params: &Option<Params>) -> Result<()>;
    fn rust_handleBeginBuild(&self, params: &Option<Params>) -> Result<()>;
    fn rust_handleDiagnosticsBegin(&self, params: &Option<Params>) -> Result<()>;
    fn rust_handleDiagnosticsEnd(&self, params: &Option<Params>) -> Result<()>;
    fn cquery_handleProgress(&self, params: &Option<Params>) -> Result<()>;
    fn cquery_base(&self, params: &Option<Params>) -> Result<Value>;
    fn cquery_derived(&self, params: &Option<Params>) -> Result<Value>;
    fn cquery_callers(&self, params: &Option<Params>) -> Result<Value>;
    fn cquery_vars(&self, params: &Option<Params>) -> Result<Value>;
}

impl ILanguageClient for Arc<Mutex<State>> {
    fn get<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&State) -> Result<T>,
    {
        let state = self.lock()
            .or_else(|_| Err(err_msg("Failed to lock state")))?;
        f(&state)
    }

    fn update<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut State) -> Result<T>,
    {
        use log::Level;

        let mut state = self.lock()
            .or_else(|_| Err(err_msg("Failed to lock state")))?;
        let before = if log_enabled!(Level::Debug) {
            let s = serde_json::to_string(state.deref())?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };
        let result = f(&mut state);
        let after = if log_enabled!(Level::Debug) {
            let s = serde_json::to_string(state.deref())?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };
        for (k, (v1, v2)) in diff_value(&before, &after, "state") {
            debug!("{}: {} ==> {}", k, v1, v2);
        }
        result
    }

    fn loop_message<T: BufRead>(&self, input: T, languageId: Option<String>) -> Result<()> {
        // Count how many consequent empty lines.
        let mut count_empty_lines = 0;

        let mut input = input;
        let mut content_length = 0;
        loop {
            let mut message = String::new();
            let mut line = String::new();
            if let Some(languageId) = languageId.clone() {
                input.read_line(&mut line)?;
                line = line.strip();
                if line.is_empty() {
                    count_empty_lines += 1;
                    if count_empty_lines > 5 {
                        if let Err(err) = self.cleanup(&languageId) {
                            error!("Error when cleanup: {:?}", err);
                        }

                        let mut message = format!("Language server ({}) exited unexpectedly!", languageId);
                        match get_log_server() {
                            Ok(log_server) => {
                                message += "\n\nlanguage server stderr:\n";
                                message += &log_server;
                            }
                            Err(err) => error!("Error when get_log_server: {:?}", err),
                        }
                        if let Err(err) = self.echoerr(&message) {
                            error!("Error in echoerr: {:?}", err);
                        };
                        bail!("{}", message);
                    }

                    let mut buf = vec![0; content_length];
                    input.read_exact(buf.as_mut_slice())?;
                    message = String::from_utf8(buf)?;
                } else {
                    count_empty_lines = 0;
                    if !line.starts_with("Content-Length") {
                        continue;
                    }

                    let tokens: Vec<&str> = line.splitn(2, ':').collect();
                    let len = tokens
                        .get(1)
                        .ok_or_else(|| format_err!("Failed to get length! tokens: {:?}", tokens))?
                        .trim();
                    content_length = usize::from_str(len)?;
                }
            } else if input.read_line(&mut message)? == 0 {
                break;
            }

            message = message.strip();
            if message.is_empty() {
                continue;
            }
            info!("<= {}", message);
            let state = Arc::clone(self);
            let languageId_clone = languageId.clone();
            let spawn_result = std::thread::Builder::new()
                .name(format!(
                    "Handler-{}",
                    languageId.clone().unwrap_or_else(|| "main".to_owned())
                ))
                .spawn(move || {
                    if let Err(err) = state.handle_message(languageId_clone, message.clone()) {
                        if err.downcast_ref::<LCError>().is_none() {
                            error!(
                                "Error handling message. Message: {}. Error: {:?}",
                                message, err
                            );
                        }
                    }
                });
            if let Err(err) = spawn_result {
                error!("Failed to spawn handler: {:?}", err);
            }
        }

        Ok(())
    }

    /// Handle an incoming message.
    fn handle_message(&self, languageId: Option<String>, message: String) -> Result<()> {
        if let Ok(output) = serde_json::from_str::<Output>(&message) {
            let tx = self.update(|state| {
                state
                    .txs
                    .remove(&output.id().to_int()?)
                    .ok_or_else(|| format_err!("Failed to get channel sender! id: {:?}", output.id()))
            })?;
            let result = match output {
                Output::Success(success) => Ok(success.result),
                Output::Failure(failure) => Err(format_err!("{}", failure.error.message)),
            };
            tx.send(result)?;
            return Ok(());
        }

        // FIXME
        let message = message.replace(r#","meta":{}"#, "");

        let call = serde_json::from_str(&message)?;

        match call {
            Call::MethodCall(method_call) => {
                let result: Result<Value> = match method_call.method.as_str() {
                    lsp::request::HoverRequest::METHOD => self.textDocument_hover(&method_call.params),
                    lsp::request::GotoDefinition::METHOD => self.textDocument_definition(&method_call.params),
                    lsp::request::Rename::METHOD => self.textDocument_rename(&method_call.params),
                    lsp::request::DocumentSymbol::METHOD => self.textDocument_documentSymbol(&method_call.params),
                    lsp::request::WorkspaceSymbol::METHOD => self.workspace_symbol(&method_call.params),
                    lsp::request::CodeActionRequest::METHOD => self.textDocument_codeAction(&method_call.params),
                    lsp::request::Completion::METHOD => self.textDocument_completion(&method_call.params),
                    lsp::request::SignatureHelpRequest::METHOD => self.textDocument_signatureHelp(&method_call.params),
                    lsp::request::References::METHOD => self.textDocument_references(&method_call.params),
                    lsp::request::Formatting::METHOD => self.textDocument_formatting(&method_call.params),
                    lsp::request::RangeFormatting::METHOD => self.textDocument_rangeFormatting(&method_call.params),
                    lsp::request::ResolveCompletionItem::METHOD => self.completionItem_resolve(&method_call.params),
                    lsp::request::ExecuteCommand::METHOD => self.workspace_executeCommand(&method_call.params),
                    lsp::request::ApplyWorkspaceEdit::METHOD => self.workspace_applyEdit(&method_call.params),
                    REQUEST__RustImplementations => self.rustDocument_implementations(&method_call.params),
                    // Extensions.
                    REQUEST__GetState => self.languageClient_getState(&method_call.params),
                    REQUEST__IsAlive => self.languageClient_isAlive(&method_call.params),
                    REQUEST__StartServer => self.languageClient_startServer(&method_call.params),
                    REQUEST__RegisterServerCommands => self.languageClient_registerServerCommands(&method_call.params),
                    REQUEST__SetLoggingLevel => self.languageClient_setLoggingLevel(&method_call.params),
                    REQUEST__OmniComplete => self.languageClient_omniComplete(&method_call.params),
                    REQUEST__CqueryBase => self.cquery_base(&method_call.params),
                    REQUEST__CqueryCallers => self.cquery_callers(&method_call.params),
                    REQUEST__CqueryDerived => self.cquery_derived(&method_call.params),
                    REQUEST__CqueryVars => self.cquery_vars(&method_call.params),
                    _ => Err(format_err!("Unknown method call: {}", method_call.method)),
                };

                if let Err(err) = result.as_ref() {
                    if err.downcast_ref::<LCError>().is_none() {
                        error!(
                            "Error handling message. Message: {}. Error: {:?}",
                            message, result
                        );
                    }
                }

                self.output(
                    languageId.as_ref().map(|s| s.as_str()),
                    method_call.id,
                    result,
                )?
            }
            Call::Notification(notification) => {
                match notification.method.as_str() {
                    lsp::notification::DidOpenTextDocument::METHOD => self.textDocument_didOpen(&notification.params)?,
                    lsp::notification::DidChangeTextDocument::METHOD => {
                        self.textDocument_didChange(&notification.params)?
                    }
                    lsp::notification::DidSaveTextDocument::METHOD => self.textDocument_didSave(&notification.params)?,
                    lsp::notification::DidCloseTextDocument::METHOD => {
                        self.textDocument_didClose(&notification.params)?
                    }
                    lsp::notification::PublishDiagnostics::METHOD => {
                        self.textDocument_publishDiagnostics(&notification.params)?
                    }
                    lsp::notification::LogMessage::METHOD => self.window_logMessage(&notification.params)?,
                    lsp::notification::Exit::METHOD => self.exit(&notification.params)?,
                    // Extensions.
                    NOTIFICATION__HandleBufReadPost => self.languageClient_handleBufReadPost(&notification.params)?,
                    NOTIFICATION__HandleTextChanged => self.languageClient_handleTextChanged(&notification.params)?,
                    NOTIFICATION__HandleBufWritePost => self.languageClient_handleBufWritePost(&notification.params)?,
                    NOTIFICATION__HandleBufDelete => self.languageClient_handleBufDelete(&notification.params)?,
                    NOTIFICATION__HandleCursorMoved => self.languageClient_handleCursorMoved(&notification.params)?,
                    NOTIFICATION__FZFSinkLocation => self.languageClient_FZFSinkLocation(&notification.params)?,
                    NOTIFICATION__FZFSinkCommand => self.languageClient_FZFSinkCommand(&notification.params)?,
                    NOTIFICATION__NCMRefresh => self.NCM_refresh(&notification.params)?,
                    // Extensions by language servers.
                    NOTIFICATION__LanguageStatus => self.language_status(&notification.params)?,
                    NOTIFICATION__RustBeginBuild => self.rust_handleBeginBuild(&notification.params)?,
                    NOTIFICATION__RustDiagnosticsBegin => self.rust_handleDiagnosticsBegin(&notification.params)?,
                    NOTIFICATION__RustDiagnosticsEnd => self.rust_handleDiagnosticsEnd(&notification.params)?,
                    NOTIFICATION__CqueryProgress => self.cquery_handleProgress(&notification.params)?,
                    _ => warn!("Unknown notification: {:?}", notification.method),
                }
            }
            Call::Invalid(id) => bail!("Invalid message of id: {:?}", id),
        }

        Ok(())
    }

    /// Send message to RPC server.
    fn write(&self, languageId: Option<&str>, message: &str) -> Result<()> {
        if let Some(languageId) = languageId {
            self.update(|state| {
                let writer = state
                    .writers
                    .get_mut(languageId)
                    .ok_or(LCError::NoLanguageServer {
                        languageId: languageId.to_owned(),
                    })?;
                write!(
                    writer,
                    "Content-Length: {}\r\n\r\n{}",
                    message.len(),
                    message
                )?;
                Ok(writer.flush()?)
            })?;
        } else {
            println!("Content-Length: {}\n\n{}", message.len(), message);
        }

        Ok(())
    }

    /// Write an RPC call output.
    fn output(&self, languageId: Option<&str>, id: Id, result: Result<Value>) -> Result<()> {
        let response = match result {
            Ok(ok) => Output::Success(Success {
                jsonrpc: Some(Version::V2),
                id,
                result: ok,
            }),
            Err(err) => Output::Failure(Failure {
                jsonrpc: Some(Version::V2),
                id,
                error: err.to_rpc_error(),
            }),
        };

        let message = serde_json::to_string(&response)?;
        info!("=> {}", message);
        self.write(languageId, &message)?;
        Ok(())
    }

    /// RPC method call.
    fn call<P: Serialize>(&self, languageId: Option<&str>, method: &str, params: P) -> Result<Value> {
        let id = self.update(|state| {
            state.id += 1;
            Ok(state.id)
        })?;

        let method_call = MethodCall {
            jsonrpc: Some(Version::V2),
            id: Id::Num(id),
            method: method.into(),
            params: Some(params.to_params()?),
        };

        let (tx, cx) = channel();
        self.update(|state| {
            state.txs.insert(id, tx);
            Ok(())
        })?;

        let message = serde_json::to_string(&method_call)?;
        info!("=> {}", message);
        self.write(languageId, &message)?;

        cx.recv_timeout(std::time::Duration::from_secs(60 * 5))?
    }

    /// RPC notification.
    fn notify<P: Serialize>(&self, languageId: Option<&str>, method: &str, params: P) -> Result<()> {
        let notification = rpc::Notification {
            jsonrpc: Some(Version::V2),
            method: method.to_owned(),
            params: Some(params.to_params()?),
        };

        let message = serde_json::to_string(&notification)?;
        info!("=> {}", message);
        self.write(languageId, &message)?;

        Ok(())
    }

    fn gather_args<E: VimExp + std::fmt::Debug, T: DeserializeOwned>(
        &self,
        exps: &[E],
        map: &Option<Params>,
    ) -> Result<T> {
        let mut map = match *map {
            None | Some(Params::None) => serde_json::map::Map::new(),
            Some(Params::Array(_)) => bail!("Params should be dict!"),
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
            self.eval(&exps_request[..])?
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

    fn sync_settings(&self) -> Result<()> {
        let (autoStart, serverCommands, mut selectionUI, trace, settingsPath, loadSettings, loggingLevel, rootMarkers): (
            u64,
            HashMap<String, Vec<String>>,
            String,
            String,
            String,
            u64,
            String,
            Option<RootMarkers>,
        ) = self.eval(
            &[
                "!!get(g:, 'LanguageClient_autoStart', 1)",
                "get(g:, 'LanguageClient_serverCommands', {})",
                "get(g:, 'LanguageClient_selectionUI', '')",
                "get(g:, 'LanguageClient_trace', 'Off')",
                "get(g:, 'LanguageClient_settingsPath', '.vim/settings.json')",
                "!!get(g:, 'LanguageClient_loadSettings', 1)",
                "get(g:, 'LanguageClient_loggingLevel', 'WARN')",
                "get(g:, 'LanguageClient_rootMarkers', v:null)",
            ][..],
        )?;
        // vimscript use 1 for true, 0 for false.
        let autoStart = autoStart == 1;
        let loadSettings = loadSettings == 1;

        let trace = match trace.to_uppercase().as_str() {
            "OFF" => TraceOption::Off,
            "MESSAGES" => TraceOption::Messages,
            "VERBOSE" => TraceOption::Verbose,
            _ => bail!("Unknown trace option: {:?}", trace),
        };

        if selectionUI == "" {
            let loaded_fzf: u64 = self.eval("get(g:, 'loaded_fzf')")?;
            if loaded_fzf == 1 {
                selectionUI = "FZF".into();
            }
        }
        let selectionUI = match selectionUI.to_uppercase().as_str() {
            "FZF" => SelectionUI::FZF,
            "" | "LOCATIONLIST" | "LOCATION-LIST" => SelectionUI::LocationList,
            _ => bail!("Unknown selectionUI option: {:?}", selectionUI),
        };

        let logger = LOGGER
            .deref()
            .as_ref()
            .or_else(|_| Err(err_msg("No logger")))?;
        logger::set_logging_level(logger, &loggingLevel)?;

        let (diagnosticsEnable, diagnosticsList, diagnosticsDisplay, windowLogMessageLevel): (
            u64,
            DiagnosticsList,
            Value,
            String,
        ) = self.eval(
            &[
                "!!get(g:, 'LanguageClient_diagnosticsEnable', v:true)",
                "get(g:, 'LanguageClient_diagnosticsList', 'Quickfix')",
                "get(g:, 'LanguageClient_diagnosticsDisplay', {})",
                "get(g:, 'LanguageClient_windowLogMessageLevel', 'Warning')",
            ][..],
        )?;
        let diagnosticsEnable = diagnosticsEnable == 1;
        let windowLogMessageLevel = match windowLogMessageLevel.to_uppercase().as_str() {
            "ERROR" => MessageType::Error,
            "WARNING" => MessageType::Warning,
            "INFO" => MessageType::Info,
            "LOG" => MessageType::Log,
            _ => bail!("Unknown windowLogMessageLevel: {}", windowLogMessageLevel),
        };

        self.update(|state| {
            state.autoStart = autoStart;
            state.serverCommands.merge(serverCommands);
            state.selectionUI = selectionUI;
            state.trace = trace;
            state.diagnosticsEnable = diagnosticsEnable;
            state.diagnosticsList = diagnosticsList;
            state.diagnosticsDisplay =
                serde_json::from_value(serde_json::to_value(&state.diagnosticsDisplay)?.combine(diagnosticsDisplay))?;
            state.windowLogMessageLevel = windowLogMessageLevel;
            state.settingsPath = settingsPath;
            state.loadSettings = loadSettings;
            state.rootMarkers = rootMarkers;
            Ok(())
        })?;

        Ok(())
    }

    fn define_signs(&self) -> Result<()> {
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

    fn apply_WorkspaceEdit(&self, edit: &WorkspaceEdit, params: &Option<Params>) -> Result<()> {
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
        debug!("End apply WorkspaceEdit");
        self.goto_location(&Some("buffer".to_string()), &filename, line, character)?;
        Ok(())
    }

    fn apply_TextEdits<P: AsRef<Path>>(&self, path: P, edits: &[TextEdit]) -> Result<()> {
        debug!("Begin apply TextEdits: {:?}", edits);
        let mut edits = edits.to_vec();
        edits.reverse();
        edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        edits.reverse();
        self.goto_location(&None, &path, 0, 0)?;
        let mut lines: Vec<String> = self.getbufline(&path)?;
        let lines_len = lines.len();
        lines = apply_TextEdits(&lines, &edits)?;
        let fixendofline: u64 = self.eval("&fixendofline")?;
        if fixendofline == 1 && lines[lines.len() - 1].is_empty() {
            lines.pop();
        }
        self.notify(None, "setline", json!([1, lines]))?;
        if lines.len() < lines_len {
            self.command(&format!("{},{}d", lines.len() + 1, lines_len))?;
        }
        debug!("End apply TextEdits");
        Ok(())
    }

    fn display_diagnostics(&self, filename: &str, diagnostics: &[Diagnostic]) -> Result<()> {
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
            state.line_diagnostics.merge(line_diagnostics);
            Ok(())
        })?;

        // Signs.
        let texts = self.get(|state| {
            let text_document = state
                .text_documents
                .get(filename)
                .ok_or_else(|| format_err!("TextDocumentItem not found! filename: {}", filename))?;
            Ok(text_document.text.clone())
        })?;
        let texts: Vec<&str> = texts.split('\n').collect();
        let mut signs: Vec<_> = diagnostics
            .iter()
            .map(|dn| {
                let line = dn.range.start.line;
                let text = texts
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

        // Quickfix.
        let qflist: Vec<_> = diagnostics
            .iter()
            .map(|dn| QuickfixEntry {
                filename: filename.to_owned(),
                lnum: dn.range.start.line + 1,
                col: Some(dn.range.start.character + 1),
                nr: dn.code.clone().map(|ns| ns.to_string()),
                text: Some(dn.message.to_owned()),
                typee: dn.severity.map(|sev| sev.to_quickfix_entry_type()),
            })
            .collect();
        let diagnosticsList = self.get(|state| Ok(state.diagnosticsList.clone()))?;
        match diagnosticsList {
            DiagnosticsList::Quickfix => {
                self.call(None, "setqflist", [qflist])?;
            }
            DiagnosticsList::Location => {
                self.call(None, "setloclist", json!([0, qflist]))?;
            }
        };

        let is_nvim: u64 = self.eval("has('nvim')")?;
        if is_nvim != 1 {
            return Ok(());
        }

        let mut source: Option<u64> = self.get(|state| Ok(state.highlight_source))?;
        if source.is_none() {
            let exp = format!(
                "nvim_buf_add_highlight({}, {}, {}, {}, {}, {})",
                0, 0, "''", 1, 1, 1
            );
            source = Some(self.eval(exp)?);
            self.update(|state| {
                state.highlight_source = source;
                Ok(())
            })?;
        }
        let source = source.ok_or_else(|| err_msg("Empty highlight source id"))?;
        let diagnosticsDisplay = self.get(|state| Ok(state.diagnosticsDisplay.clone()))?;

        // Highlight.
        // TODO: Optimize.
        self.call(None, "nvim_buf_clear_highlight", json!([0, source, 1, -1]))?;
        for dn in diagnostics.iter() {
            let severity = dn.severity.unwrap_or(DiagnosticSeverity::Information);
            let hl_group = diagnosticsDisplay
                .get(&severity.to_int()?)
                .ok_or_else(|| err_msg("Failed to get display"))?
                .texthl
                .clone();
            self.notify(
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

    fn display_locations(&self, locations: &[Location], _languageId: &str) -> Result<()> {
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

                self.notify(
                    None,
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::LocationList => {
                let loclist: Result<Vec<_>> = locations
                    .iter()
                    .map(|loc| {
                        let filename = loc.uri.filepath()?;
                        let start = loc.range.start;
                        let text = self.get_line(&filename, start.line).unwrap_or_default();
                        Ok(json!({
                            "filename": filename,
                            "lnum": start.line + 1,
                            "col": start.character + 1,
                            "text": text,
                        }))
                    })
                    .collect();
                let loclist = loclist?;

                self.notify(None, "setloclist", json!([0, loclist]))?;
                self.echo("Location list updated.")?;
            }
        }
        Ok(())
    }

    fn languageClient_getState(&self, _params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__GetState);
        let s = self.get(|state| Ok(serde_json::to_string(state)?))?;
        info!("End {}", REQUEST__GetState);
        Ok(Value::String(s))
    }

    fn languageClient_isAlive(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__IsAlive);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let is_alive = self.get(|state| Ok(state.writers.contains_key(&languageId)))?;
        info!("End {}", REQUEST__IsAlive);
        Ok(Value::Bool(is_alive))
    }

    fn languageClient_startServer(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__StartServer);
        let (cmdargs,): (Vec<String>,) = self.gather_args(&[("cmdargs", "[]")], params)?;
        let cmdparams = vim_cmd_args_to_value(&cmdargs)?;
        let params = &Some(params.clone().to_value().combine(cmdparams).to_params()?);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Filename],
            params,
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
                        "No language server command found for type: {}.",
                        &languageId
                    )
                })
        })?;

        let home = env::home_dir().ok_or_else(|| err_msg("Failed to get home dir"))?;
        let home = home.to_str()
            .ok_or_else(|| err_msg("Failed to convert PathBuf to str"))?;
        let command: Vec<_> = command
            .into_iter()
            .map(|cmd| {
                if cmd.starts_with('~') {
                    cmd.replacen('~', home, 1)
                } else {
                    cmd
                }
            })
            .collect();

        let stderr = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&get_logpath_server())?;

        let process = std::process::Command::new(command.get(0).ok_or_else(|| err_msg("Empty command!"))?)
            .args(&command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()?;

        let child_id = process.id();
        let reader = BufReader::new(process
            .stdout
            .ok_or_else(|| err_msg("Failed to get subprocess stdout"))?);
        let writer = BufWriter::new(process
            .stdin
            .ok_or_else(|| err_msg("Failed to get subprocess stdin"))?);

        self.update(|state| {
            state.child_ids.insert(languageId.clone(), child_id);
            state.writers.insert(languageId.clone(), writer);
            Ok(())
        })?;

        let state = Arc::clone(self);
        let languageId_clone = languageId.clone();
        let thread_name = format!("RPC-{}", languageId);
        std::thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || {
                if let Err(err) = state.loop_message(reader, Some(languageId_clone)) {
                    error!("{} thread error: {}", thread_name, err);
                }
            })?;

        info!("End {}", REQUEST__StartServer);

        if self.get(|state| Ok(state.writers.len()))? == 1 {
            self.define_signs()?;
        }

        self.initialize(params)?;
        self.textDocument_didOpen(params)?;
        self.textDocument_didChange(params)?;

        if self.eval::<_, u64>("exists('#User#LanguageClientStarted')")? == 1 {
            self.command("doautocmd User LanguageClientStarted")?;
        }
        Ok(Value::Null)
    }

    // TODO: verify.
    fn languageClient_registerServerCommands(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__RegisterServerCommands);
        let params = params.clone().ok_or_else(|| err_msg("Empty params!"))?;
        let map = match params {
            Params::Map(map) => Value::Object(map),
            _ => bail!("Unexpected params type!"),
        };
        let map = serde_json::from_value(map)?;
        self.update(|state| Ok(state.serverCommands.merge(map)))?;
        let exp = format!(
            "let g:LanguageClient_serverCommands={}",
            serde_json::to_string(&self.get(|state| Ok(state.serverCommands.clone()))?)?
        );
        self.command(&exp)?;
        info!("End {}", REQUEST__RegisterServerCommands);
        Ok(Value::Null)
    }

    fn languageClient_setLoggingLevel(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__SetLoggingLevel);
        let (loggingLevel,): (String,) = self.gather_args(&["loggingLevel"], params)?;
        let logger = LOGGER
            .deref()
            .as_ref()
            .or_else(|_| Err(err_msg("No logger")))?;
        logger::set_logging_level(logger, &loggingLevel)?;
        info!("End {}", REQUEST__SetLoggingLevel);
        Ok(Value::Null)
    }

    fn languageClient_handleBufReadPost(&self, params: &Option<Params>) -> Result<()> {
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
            return Ok(());
        }

        if self.get(|state| Ok(state.writers.contains_key(&languageId)))? {
            // Language server is running but file is not within project root.
            let is_in_root = self.get(|state| {
                let root = state
                    .roots
                    .get(&languageId)
                    .ok_or_else(|| format_err!("Failed to get root! languageId: {}", languageId))?;
                Ok(filename.starts_with(root))
            })?;
            if !is_in_root {
                return Ok(());
            }

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
            let autoStart: i32 = self.eval("!!get(g:, 'LanguageClient_autoStart', v:true)")?;
            if autoStart == 1 {
                if let Err(err) = self.languageClient_startServer(params) {
                    warn!("{}", err);
                }
            }
        }

        info!("End {}", NOTIFICATION__HandleBufReadPost);
        Ok(())
    }

    fn languageClient_handleTextChanged(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__HandleTextChanged);
        self.textDocument_didChange(params)?;
        info!("End {}", NOTIFICATION__HandleTextChanged);
        Ok(())
    }

    fn languageClient_handleBufWritePost(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__HandleBufWritePost);
        self.textDocument_didSave(params)?;
        info!("End {}", NOTIFICATION__HandleBufWritePost);
        Ok(())
    }

    fn languageClient_handleBufDelete(&self, params: &Option<Params>) -> Result<()> {
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

    fn languageClient_handleCursorMoved(&self, params: &Option<Params>) -> Result<()> {
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

    fn initialize(&self, params: &Option<Params>) -> Result<Value> {
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
        let root = match rootPath {
            Some(r) => r,
            _ => {
                let rootMarkers = self.get(|state| Ok(state.rootMarkers.clone()))?;
                get_rootPath(Path::new(&filename), &languageId, &rootMarkers)?
                    .to_str()
                    .ok_or_else(|| err_msg("Failed to convert &Path to &str"))?
                    .to_owned()
            }
        };
        info!("Project root: {}", root);
        let has_snippet_support = has_snippet_support > 0;
        self.update(|state| Ok(state.roots.insert(languageId.clone(), root.clone())))?;

        let settings = || -> Result<Value> {
            if !self.get(|state| Ok(state.loadSettings))? {
                return Ok(json!({}));
            }

            let mut f = File::open(Path::new(&root).join(self.get(|state| Ok(state.settingsPath.clone()))?))?;
            let mut buffer = String::new();
            f.read_to_string(&mut buffer)?;
            Ok(serde_json::from_str(&buffer)?)
        }()
            .unwrap_or_else(|_| json!({}));
        debug!("Project settings: {}", serde_json::to_string(&settings)?);
        let initialization_options = Some(settings["initializationOptions"].clone());
        debug!(
            "Project settings.initializationOptions: {}",
            serde_json::to_string(&initialization_options)?
        );

        let result = self.call(
            Some(&languageId),
            lsp::request::Initialize::METHOD,
            InitializeParams {
                process_id: Some(unsafe { libc::getpid() } as u64),
                root_path: Some(root.clone()),
                root_uri: Some(root.to_url()?),
                initialization_options,
                capabilities: ClientCapabilities {
                    workspace: None,
                    text_document: Some(TextDocumentClientCapabilities {
                        synchronization: None,
                        completion: Some(CompletionCapability {
                            dynamic_registration: None,
                            completion_item: Some(CompletionItemCapability {
                                snippet_support: Some(has_snippet_support),
                                commit_characters_support: None,
                                documentation_format: None,
                            }),
                        }),
                        hover: None,
                        signature_help: None,
                        references: None,
                        document_highlight: None,
                        document_symbol: None,
                        formatting: None,
                        range_formatting: None,
                        on_type_formatting: None,
                        definition: None,
                        code_action: None,
                        code_lens: None,
                        document_link: None,
                        rename: None,
                    }),
                    experimental: None,
                },
                trace: TraceOption::default(),
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
            let message = "LanguageClient: failed to register as NCM source!";
            debug!("{}: {:?}", message, e);
            self.echoerr(message)?;
        }
        Ok(result)
    }

    fn registerCMSource(&self, languageId: &str, result: &Value) -> Result<()> {
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
                    .iter()
                    .map(|c| regex::escape(c))
                    .collect();
                strings
            })
            .unwrap_or_default();

        self.notify(
            None,
            "cm#register_source",
            json!([{
                "name": format!("LanguageClient_{}", languageId),
                "priority": 9,
                "scopes": [languageId],
                "cm_refresh_patterns": trigger_patterns,
                "abbreviation": "LC",
                "cm_refresh": NOTIFICATION__NCMRefresh,
            }]),
        )?;
        info!("End register NCM source");
        Ok(())
    }

    fn get_line<P: AsRef<Path>>(&self, path: P, line: u64) -> Result<String> {
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

        Ok(text.strip())
    }

    fn NCM_refresh(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__NCMRefresh);
        let params = match *params {
            None | Some(Params::None) => bail!("Empty params!"),
            Some(Params::Map(_)) => bail!("Expecting array. Got dict."),
            Some(Params::Array(ref arr)) => Value::Array(arr.clone()),
        };
        let (info, ctx): (NCMInfo, NCMContext) = serde_json::from_value(params)?;
        if ctx.typed.is_empty() {
            return Ok(());
        }

        let result = self.textDocument_completion(&Some(json!({
                "line": ctx.lnum - 1,
                "character": ctx.col - 1,
            }).to_params()?))?;
        let result: Option<CompletionResponse> = serde_json::from_value(result)?;
        let result = result.unwrap_or_else(|| CompletionResponse::Array(vec![]));
        let is_incomplete = match result {
            CompletionResponse::Array(_) => false,
            CompletionResponse::List(ref list) => list.is_incomplete,
        };
        let matches: Vec<VimCompleteItem> = match result {
            CompletionResponse::Array(arr) => arr,
            CompletionResponse::List(list) => list.items,
        }.into_iter()
            .map(|lspitem| lspitem.into())
            .collect();
        self.notify(
            None,
            "cm#complete",
            json!([info.name, ctx, ctx.startcol, matches, is_incomplete]),
        )?;
        info!("End {}", NOTIFICATION__NCMRefresh);
        Ok(())
    }

    fn languageClient_omniComplete(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__OmniComplete);
        let result = self.textDocument_completion(params)?;
        let result: Option<CompletionResponse> = serde_json::from_value(result)?;
        let result = result.unwrap_or_else(|| CompletionResponse::Array(vec![]));
        let matches: Vec<VimCompleteItem> = match result {
            CompletionResponse::Array(arr) => arr,
            CompletionResponse::List(list) => list.items,
        }.into_iter()
            .map(|lspitem| lspitem.into())
            .collect();
        info!("End {}", REQUEST__OmniComplete);
        Ok(serde_json::to_value(matches)?)
    }

    fn textDocument_references(&self, params: &Option<Params>) -> Result<Value> {
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
        self.display_locations(&locations, &languageId)?;

        info!("End {}", lsp::request::References::METHOD);
        Ok(result)
    }

    fn textDocument_formatting(&self, params: &Option<Params>) -> Result<Value> {
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

        let (tab_size, insert_spaces): (u64, u64) = self.eval(&["&tabstop", "&expandtab"][..])?;
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

    fn textDocument_rangeFormatting(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::RangeFormatting::METHOD);
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

        let (tab_size, insert_spaces, start_line, end_line, end_character): (u64, u64, u64, u64, u64) = self.eval(
            &[
                "&tabstop",
                "&expandtab",
                "v:lnum - 1",
                "v:lnum - 1 + v:count",
                "len(getline(v:lnum + v:count)) - 1",
            ][..],
        )?;
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
                        character: end_character,
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

    fn completionItem_resolve(&self, params: &Option<Params>) -> Result<Value> {
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

    fn textDocument_didOpen(&self, params: &Option<Params>) -> Result<()> {
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

        info!("End {}", lsp::notification::DidOpenTextDocument::METHOD);
        Ok(())
    }

    fn textDocument_didChange(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::DidChangeTextDocument::METHOD);
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
        if !self.get(|state| Ok(state.text_documents.contains_key(&filename)))? {
            warn!("Not opened yet. Switching to didOpen.");
            return self.textDocument_didOpen(params);
        }

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
            let document = state
                .text_documents
                .get_mut(&filename)
                .ok_or_else(|| format_err!("Failed to get TextDocumentItem! filename: {}", filename))?;

            let version = document.version + 1;
            document.version = version;
            document.text = text.clone();
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
                content_changes: vec![
                    TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text,
                    },
                ],
            },
        )?;

        info!("End {}", lsp::notification::DidChangeTextDocument::METHOD);
        Ok(())
    }

    fn textDocument_didSave(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::DidSaveTextDocument::METHOD);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[VimVar::Buftype, VimVar::LanguageId, VimVar::Filename],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }

        self.notify(
            Some(&languageId),
            lsp::notification::DidSaveTextDocument::METHOD,
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
            },
        )?;

        info!("End {}", lsp::notification::DidSaveTextDocument::METHOD);
        Ok(())
    }

    fn textDocument_didClose(&self, params: &Option<Params>) -> Result<()> {
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

    fn textDocument_publishDiagnostics(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::PublishDiagnostics::METHOD);
        let params: PublishDiagnosticsParams = serde_json::from_value(params.clone().to_value())?;
        if !self.get(|state| Ok(state.diagnosticsEnable))? {
            return Ok(());
        }

        let mut filename = params
            .uri
            .filepath()?
            .to_str()
            .ok_or_else(|| err_msg("Failed to convert PathBuf to str"))?
            .to_owned();
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

        info!("End {}", lsp::notification::PublishDiagnostics::METHOD);

        let current_filename: String = self.eval(VimVar::Filename)?;
        if filename != current_filename.canonicalize() {
            return Ok(());
        }

        self.display_diagnostics(&current_filename, &params.diagnostics)?;
        self.languageClient_handleCursorMoved(&None)?;

        if self.eval::<_, u64>("exists('#User#LanguageClientDiagnosticsChanged')")? == 1 {
            self.command("doautocmd User LanguageClientDiagnosticsChanged")?;
        }

        Ok(())
    }

    fn window_logMessage(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::LogMessage::METHOD);
        let params: LogMessageParams = serde_json::from_value(params.clone().to_value())?;
        let threshold = self.get(|state| state.windowLogMessageLevel.to_int())?;
        if params.typ.to_int()? > threshold {
            return Ok(());
        }

        let msg = format!("[{:?}] {}", params.typ, params.message);
        self.echomsg(&msg)?;
        info!("End {}", lsp::notification::LogMessage::METHOD);
        Ok(())
    }

    fn textDocument_hover(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::HoverRequest::METHOD);
        let (languageId, filename, line, character, handle): (String, String, u64, u64, bool) = self.gather_args(
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
            let message = hover.to_string();
            self.echo(&message)?;
        }

        info!("End {}", lsp::request::HoverRequest::METHOD);
        Ok(result)
    }

    fn textDocument_definition(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::GotoDefinition::METHOD);
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
            lsp::request::GotoDefinition::METHOD,
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

        let response: GotoDefinitionResponse = serde_json::from_value(result.clone())?;

        match response {
            GotoDefinitionResponse::None => {
                self.echowarn("Not found!")?;
                return Ok(Value::Null);
            }
            GotoDefinitionResponse::Scalar(loc) => {
                self.goto_location(
                    &goto_cmd,
                    loc.uri.filepath()?,
                    loc.range.start.line,
                    loc.range.start.character,
                )?;
            }
            GotoDefinitionResponse::Array(arr) => match arr.len() {
                0 => self.echowarn("Not found!")?,
                1 => {
                    let loc = arr.get(0).ok_or_else(|| err_msg("Not found!"))?;
                    self.goto_location(
                        &goto_cmd,
                        loc.uri.filepath()?,
                        loc.range.start.line,
                        loc.range.start.character,
                    )?;
                }
                _ => self.display_locations(&arr, &languageId)?,
            },
        };

        info!("End {}", lsp::request::GotoDefinition::METHOD);
        Ok(result)
    }

    fn textDocument_rename(&self, params: &Option<Params>) -> Result<Value> {
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

    fn textDocument_documentSymbol(&self, params: &Option<Params>) -> Result<Value> {
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

                self.notify(
                    None,
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::LocationList => {
                let loclist: Vec<_> = symbols
                    .iter()
                    .map(|sym| {
                        let start = sym.location.range.start;
                        json!({
                            "filename": filename,
                            "lnum": start.line + 1,
                            "col": start.character + 1,
                            "text": sym.name,
                        })
                    })
                    .collect();

                self.notify(None, "setloclist", json!([0, loclist]))?;
                self.echo("Document symbols populated to location list.")?;
            }
        }

        info!("End {}", lsp::request::DocumentSymbol::METHOD);
        Ok(result)
    }

    fn workspace_symbol(&self, params: &Option<Params>) -> Result<Value> {
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

                self.notify(
                    None,
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::LocationList => {
                let loclist: Vec<_> = symbols
                    .iter()
                    .map(|sym| {
                        let start = sym.location.range.start;
                        json!({
                        "filename": sym.location.uri.to_file_path(),
                        "lnum": start.line + 1,
                        "col": start.character + 1,
                        "text": sym.name,
                    })
                    })
                    .collect();

                self.notify(None, "setloclist", json!([0, loclist]))?;
                self.echo("Workspace symbols populated to location list.")?;
            }
        }

        info!("End {}", lsp::request::WorkspaceSymbol::METHOD);
        Ok(result)
    }

    fn languageClient_FZFSinkLocation(&self, params: &Option<Params>) -> Result<()> {
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
            let languageId: String = self.eval(VimVar::LanguageId)?;
            let root = self.get(|state| {
                state
                    .roots
                    .get(&languageId)
                    .cloned()
                    .ok_or_else(|| format_err!("Failed to get root! languageId: {}", languageId))
            })?;
            Path::new(&root)
                .join(relpath)
                .to_str()
                .ok_or_else(|| err_msg("Failed to convert PathBuf to str"))?
                .to_owned()
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

        self.goto_location(&None, &filename, line, character)?;

        info!("End {}", NOTIFICATION__FZFSinkLocation);
        Ok(())
    }

    fn languageClient_FZFSinkCommand(&self, params: &Option<Params>) -> Result<()> {
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
            state
                .stashed_codeAction_commands
                .iter()
                .find(|e| e.command == command && e.title == title)
                .cloned()
                .ok_or_else(|| err_msg("No stashed command found!"))
        })?;

        if self.try_handle_command_by_client(&entry)? {
            return Ok(());
        }

        self.workspace_executeCommand(&Some(json!({
                "command": entry.command,
                "arguments": entry.arguments,
            }).to_params()?))?;

        self.update(|state| {
            state.stashed_codeAction_commands = vec![];
            Ok(())
        })?;

        info!("End {}", NOTIFICATION__FZFSinkCommand);
        Ok(())
    }

    fn try_handle_command_by_client(&self, cmd: &Command) -> Result<bool> {
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

    fn textDocument_codeAction(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::CodeActionRequest::METHOD);
        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
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

        let diagnostics: Vec<_> = self.get(|state| {
            Ok(state
                .diagnostics
                .get(&filename)
                .ok_or_else(|| err_msg("No diagnostics found!"))?
                .iter()
                .filter(|dn| {
                    let start = dn.range.start;
                    let end = dn.range.end;
                    start.line <= line && start.character <= character && end.line >= line && end.character >= character
                })
                .cloned()
                .collect())
        })?;
        let result = self.call(
            Some(&languageId),
            lsp::request::CodeActionRequest::METHOD,
            CodeActionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                //TODO: is this correct?
                range: diagnostics
                    .get(0)
                    .ok_or_else(|| err_msg("No diagnostics found!"))?
                    .range,
                context: CodeActionContext { diagnostics },
            },
        )?;

        if !handle {
            return Ok(result);
        }

        let commands: Vec<Command> = serde_json::from_value(result.clone())?;

        let source: Vec<_> = commands
            .iter()
            .map(|cmd| format!("{}: {}", cmd.command, cmd.title))
            .collect();

        self.update(|state| {
            state.stashed_codeAction_commands = commands;
            Ok(())
        })?;

        self.notify(
            None,
            "s:FZF",
            json!([source, format!("s:{}", NOTIFICATION__FZFSinkCommand)]),
        )?;

        info!("End {}", lsp::request::CodeActionRequest::METHOD);
        Ok(result)
    }

    fn textDocument_completion(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::Completion::METHOD);

        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
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

    fn textDocument_signatureHelp(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::SignatureHelpRequest::METHOD);
        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
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

    fn workspace_executeCommand(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::ExecuteCommand::METHOD);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;
        let (command, arguments): (String, Vec<Value>) = self.gather_args(&["command", "arguments"], params)?;

        let result = self.call(
            Some(&languageId),
            lsp::request::ExecuteCommand::METHOD,
            ExecuteCommandParams { command, arguments },
        )?;
        info!("End {}", lsp::request::ExecuteCommand::METHOD);
        Ok(result)
    }

    fn workspace_applyEdit(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", lsp::request::ApplyWorkspaceEdit::METHOD);

        let params: ApplyWorkspaceEditParams = serde_json::from_value(params.clone().to_value())?;
        self.apply_WorkspaceEdit(&params.edit, &None)?;

        info!("End {}", lsp::request::ApplyWorkspaceEdit::METHOD);

        Ok(serde_json::to_value(ApplyWorkspaceEditResponse {
            applied: true,
        })?)
    }

    fn rustDocument_implementations(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__RustImplementations);
        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
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
        self.display_locations(&locations, &languageId)?;

        info!("End {}", REQUEST__RustImplementations);
        Ok(result)
    }

    fn exit(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", lsp::notification::Exit::METHOD);
        let (languageId,): (String,) = self.gather_args(&[VimVar::LanguageId], params)?;

        self.notify(
            Some(&languageId),
            lsp::notification::Exit::METHOD,
            Value::Null,
        )?;
        self.cleanup(&languageId)?;
        info!("End {}", lsp::notification::Exit::METHOD);
        Ok(())
    }

    fn cleanup(&self, languageId: &str) -> Result<()> {
        self.update(|state| {
            state.child_ids.remove(languageId);
            state.last_cursor_line = 0;
            Ok(())
        })?;

        let signsmap = self.update(|state| {
            state.writers.remove(languageId);
            let root = state
                .roots
                .remove(languageId)
                .ok_or_else(|| format_err!("No project root found! languageId: {}", languageId))?;

            state.text_documents.retain(|f, _| !f.starts_with(&root));
            state.diagnostics.retain(|f, _| !f.starts_with(&root));

            let mut signsmap = HashMap::new();
            state.signs.retain(|f, s| {
                if f.starts_with(&root) {
                    signsmap.insert(f.clone(), s.clone());
                    false
                } else {
                    true
                }
            });
            state
                .line_diagnostics
                .retain(|fl, _| !fl.0.starts_with(&root));
            Ok(signsmap)
        })?;

        for (filename, signs) in signsmap {
            let (_, cmd) = get_command_update_signs(&signs, &[], &filename);
            self.command(&cmd)?;
        }

        let hlsource = self.update(|state| {
            state
                .highlight_source
                .ok_or_else(|| err_msg("No highlight source"))
        });
        if let Ok(hlsource) = hlsource {
            self.call(
                None,
                "nvim_buf_clear_highlight",
                json!([0, hlsource, 1, -1]),
            )?;
        }

        if self.eval::<_, u64>("exists('#User#LanguageClientStopped')")? == 1 {
            self.command("doautocmd User LanguageClientStopped")?;
        }
        self.command(&format!("let {}=0", VIM__ServerStatus))?;
        self.command(&format!("let {}=''", VIM__ServerStatusMessage))?;
        Ok(())
    }

    fn language_status(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__LanguageStatus);
        let params: LanguageStatusParams = serde_json::from_value(params.clone().to_value())?;
        let msg = format!("{} {}", params.typee, params.message);
        self.echomsg(&msg)?;
        info!("End {}", NOTIFICATION__LanguageStatus);
        Ok(())
    }

    fn rust_handleBeginBuild(&self, _params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__RustBeginBuild);
        self.command(&format!(
            "let {}=1 | let {}='Rust: build begin'",
            VIM__ServerStatus, VIM__ServerStatusMessage
        ))?;
        info!("End {}", NOTIFICATION__RustBeginBuild);
        Ok(())
    }

    fn rust_handleDiagnosticsBegin(&self, _params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__RustDiagnosticsBegin);
        self.command(&format!(
            "let {}=1 | let {}='Rust: diagnostics begin'",
            VIM__ServerStatus, VIM__ServerStatusMessage
        ))?;
        info!("End {}", NOTIFICATION__RustDiagnosticsBegin);
        Ok(())
    }

    fn rust_handleDiagnosticsEnd(&self, _params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__RustDiagnosticsEnd);
        self.command(&format!(
            "let {}=0 | let {}='Rust: diagnostics end'",
            VIM__ServerStatus, VIM__ServerStatusMessage
        ))?;
        info!("End {}", NOTIFICATION__RustDiagnosticsEnd);
        Ok(())
    }

    fn cquery_base(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__CqueryBase);

        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
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
            REQUEST__CqueryBase,
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
        self.display_locations(&locations, &languageId)?;

        info!("End {}", REQUEST__CqueryBase);
        Ok(result)
    }

    fn cquery_derived(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__CqueryDerived);

        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
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
            REQUEST__CqueryDerived,
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
        self.display_locations(&locations, &languageId)?;

        info!("End {}", REQUEST__CqueryDerived);
        Ok(result)
    }

    fn cquery_callers(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__CqueryCallers);

        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
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
            REQUEST__CqueryCallers,
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
        self.display_locations(&locations, &languageId)?;

        info!("End {}", REQUEST__CqueryCallers);
        Ok(result)
    }

    fn cquery_vars(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__CqueryVars);

        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
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
            REQUEST__CqueryVars,
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
        self.display_locations(&locations, &languageId)?;

        info!("End {}", REQUEST__CqueryVars);
        Ok(result)
    }

    fn cquery_handleProgress(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__CqueryProgress);
        let params: CqueryProgressParams = serde_json::from_value(params.clone().to_value())?;
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
}
