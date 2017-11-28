use std;
use std::slice::SliceConcatExt;
use serde_json;
use regex;
use types::*;
use utils::*;
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
    fn gather_args<E: ToVimExp + ToString + std::fmt::Debug, T: DeserializeOwned>(
        &self,
        exps: &[E],
        params: &Option<Params>,
    ) -> Result<T>;
    fn sync_settings(&self) -> Result<()>;
    fn define_signs(&self) -> Result<()>;
    fn apply_WorkspaceEdit(&self, edit: &WorkspaceEdit) -> Result<()>;
    fn apply_TextEdits(&self, filename: &str, edits: &[TextEdit]) -> Result<()>;
    fn display_diagnostics(&self, filename: &str, diagnostics: &[Diagnostic]) -> Result<()>;
    fn registerCMSource(&self, languageId: &str, result: &Value) -> Result<()>;
    fn get_line(&self, filename: &str, line: u64) -> Result<String>;

    fn initialize(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_hover(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_definition(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_rename(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_documentSymbol(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_codeAction(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_completion(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_references(&self, params: &Option<Params>) -> Result<Value>;
    fn workspace_symbol(&self, params: &Option<Params>) -> Result<Value>;
    fn workspace_executeCommand(&self, params: &Option<Params>) -> Result<Value>;
    fn workspace_applyEdit(&self, params: &Option<Params>) -> Result<Value>;
    fn textDocument_didOpen(&self, params: &Option<Params>) -> Result<()>;
    fn textDocument_didChange(&self, params: &Option<Params>) -> Result<()>;
    fn textDocument_didSave(&self, params: &Option<Params>) -> Result<()>;
    fn textDocument_didClose(&self, params: &Option<Params>) -> Result<()>;
    fn textDocument_publishDiagnostics(&self, params: &Option<Params>) -> Result<()>;
    fn exit(&self, params: &Option<Params>) -> Result<()>;

    // Extensions.
    fn hello(&self, params: &Option<Params>) -> Result<Value>;
    fn languageClient_getState(&self, &Option<Params>) -> Result<Value>;
    fn languageClient_isAlive(&self, &Option<Params>) -> Result<Value>;
    fn languageClient_startServer(&self, params: &Option<Params>) -> Result<Value>;
    fn languageClient_registerServerCommands(&self, params: &Option<Params>) -> Result<Value>;
    fn languageClient_omniComplete(&self, params: &Option<Params>) -> Result<Value>;
    fn languageClient_handleBufReadPost(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_handleTextChanged(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_handleBufWritePost(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_handleBufDelete(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_handleCursorMoved(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_FZFSinkLocation(&self, params: &Option<Params>) -> Result<()>;
    fn languageClient_FZFSinkCommand(&self, params: &Option<Params>) -> Result<()>;
    fn NCM_refresh(&self, params: &Option<Params>) -> Result<()>;
}

impl ILanguageClient for Arc<Mutex<State>> {
    fn get<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&State) -> Result<T>,
    {
        let state = self.lock().or(Err(format_err!("Failed to lock state")))?;
        f(&state)
    }

    fn update<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut State) -> Result<T>,
    {
        use diff::chars;
        use log::LogLevel;

        let mut state = self.lock().or(Err(format_err!("Failed to lock state")))?;
        let before = if log_enabled!(LogLevel::Debug) {
            format!("{:?}", state.deref())
        } else {
            "{}".to_owned()
        };
        let result = f(&mut state);
        let after = if log_enabled!(LogLevel::Debug) {
            format!("{:?}", state.deref())
        } else {
            "{}".to_owned()
        };
        debug!("{}", chars(before.as_str(), after.as_str()).to_string());
        result
    }

    fn loop_message<T: BufRead>(&self, input: T, languageId: Option<String>) -> Result<()> {
        let mut input = input;
        let mut content_length = 0;
        loop {
            let mut message = String::new();
            let mut line = String::new();
            if languageId.is_some() {
                input.read_line(&mut line)?;
                line = line.as_str().strip().to_owned();
                if line.is_empty() {
                    let mut buf = vec![0; content_length];
                    input.read_exact(buf.as_mut_slice())?;
                    message = String::from_utf8(buf)?;
                } else {
                    if !line.starts_with("Content-Length") {
                        continue;
                    }

                    let tokens: Vec<&str> = line.splitn(2, ':').collect();
                    let len = tokens
                        .get(1)
                        .ok_or(format_err!("Failed to get length token"))?
                        .trim();
                    content_length = usize::from_str(len)?;
                }
            } else if input.read_line(&mut message)? == 0 {
                break;
            }

            message = message.as_str().strip().to_owned();
            if message.is_empty() {
                continue;
            }
            info!("<= {}", message);
            let state = Arc::clone(self);
            let languageId_clone = languageId.clone();
            let spawn_result = std::thread::Builder::new()
                .name(format!(
                    "Handler-{}",
                    languageId.clone().unwrap_or_else(|| "main".into())
                ))
                .spawn(move || {
                    if let Err(err) = state.handle_message(languageId_clone, message.clone()) {
                        if err.downcast_ref::<LCError>().is_none() {
                            error!(
                                "Error handling message. Message: {}. Error: {:?}",
                                message,
                                err
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
        if let Ok(output) = serde_json::from_str::<Output>(message.as_str()) {
            let tx = self.update(|state| {
                state
                    .txs
                    .remove(&output.id().to_int()?)
                    .ok_or(format_err!("Failed to get sender"))
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

        let call = serde_json::from_str(message.as_str())?;

        match call {
            Call::MethodCall(method_call) => {
                let result: Result<Value> = match method_call.method.as_str() {
                    REQUEST__Hover => self.textDocument_hover(&method_call.params),
                    REQUEST__GotoDefinition => self.textDocument_definition(&method_call.params),
                    REQUEST__Rename => self.textDocument_rename(&method_call.params),
                    REQUEST__DocumentSymbols => self.textDocument_documentSymbol(&method_call.params),
                    REQUEST__WorkspaceSymbols => self.workspace_symbol(&method_call.params),
                    REQUEST__CodeAction => self.textDocument_codeAction(&method_call.params),
                    REQUEST__Completion => self.textDocument_completion(&method_call.params),
                    REQUEST__References => self.textDocument_references(&method_call.params),
                    REQUEST__ExecuteCommand => self.workspace_executeCommand(&method_call.params),
                    REQUEST__ApplyEdit => self.workspace_applyEdit(&method_call.params),
                    // Extensions.
                    REQUEST__Hello => self.hello(&method_call.params),
                    REQUEST__GetState => self.languageClient_getState(&method_call.params),
                    REQUEST__IsAlive => self.languageClient_isAlive(&method_call.params),
                    REQUEST__StartServer => self.languageClient_startServer(&method_call.params),
                    REQUEST__RegisterServerCommands => self.languageClient_registerServerCommands(&method_call.params),
                    REQUEST__OmniComplete => self.languageClient_omniComplete(&method_call.params),
                    _ => Err(format_err!("Unknown method call: {}", method_call.method)),
                };

                if let Err(err) = result.as_ref() {
                    if err.downcast_ref::<LCError>().is_none() {
                        error!(
                            "Error handling message. Message: {}. Error: {:?}",
                            message,
                            result
                        );
                    }
                }

                self.output(languageId.as_ref_str(), method_call.id, result)?
            }
            Call::Notification(notification) => match notification.method.as_str() {
                NOTIFICATION__DidOpenTextDocument => self.textDocument_didOpen(&notification.params)?,
                NOTIFICATION__DidChangeTextDocument => self.textDocument_didChange(&notification.params)?,
                NOTIFICATION__DidSaveTextDocument => self.textDocument_didSave(&notification.params)?,
                NOTIFICATION__DidCloseTextDocument => self.textDocument_didClose(&notification.params)?,
                NOTIFICATION__PublishDiagnostics => self.textDocument_publishDiagnostics(&notification.params)?,
                NOTIFICATION__Exit => self.exit(&notification.params)?,
                // Extensions.
                NOTIFICATION__HandleBufReadPost => self.languageClient_handleBufReadPost(&notification.params)?,
                NOTIFICATION__HandleTextChanged => self.languageClient_handleTextChanged(&notification.params)?,
                NOTIFICATION__HandleBufWritePost => self.languageClient_handleBufWritePost(&notification.params)?,
                NOTIFICATION__HandleBufDelete => self.languageClient_handleBufDelete(&notification.params)?,
                NOTIFICATION__HandleCursorMoved => self.languageClient_handleCursorMoved(&notification.params)?,
                NOTIFICATION__FZFSinkLocation => self.languageClient_FZFSinkLocation(&notification.params)?,
                NOTIFICATION__FZFSinkCommand => self.languageClient_FZFSinkCommand(&notification.params)?,
                _ => warn!("Unknown notification: {:?}", notification.method),
            },
            Call::Invalid(id) => return Err(format_err!("Invalid message of id: {:?}", id)),
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
            println!("{}", message);
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
        self.write(languageId, message.as_str())?;
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

        let message = serde_json::to_string(&method_call)?;
        info!("=> {}", message);
        self.write(languageId, message.as_str())?;

        let (tx, cx) = channel();
        self.update(|state| {
            state.txs.insert(id, tx);
            Ok(())
        })?;

        cx.recv_timeout(std::time::Duration::from_secs(60 * 5))?
    }

    /// RPC notification.
    fn notify<P: Serialize>(&self, languageId: Option<&str>, method: &str, params: P) -> Result<()> {
        let notification = Notification {
            jsonrpc: Some(Version::V2),
            method: method.to_owned(),
            params: Some(params.to_params()?),
        };

        let message = serde_json::to_string(&notification)?;
        info!("=> {}", message);
        self.write(languageId, message.as_str())?;

        Ok(())
    }

    fn gather_args<E: ToVimExp + ToString + std::fmt::Debug, T: DeserializeOwned>(
        &self,
        exps: &[E],
        map: &Option<Params>,
    ) -> Result<T> {
        debug!("Begin gather_args");
        let mut map = match *map {
            None | Some(Params::None) => serde_json::map::Map::new(),
            Some(Params::Array(_)) => return Err(format_err!("Params should be dict!")),
            Some(Params::Map(ref map)) => map.clone(),
        };
        let mut keys_request = vec![];
        let mut exps_request = vec![];
        for e in exps {
            let k = e.to_string();
            if !map.contains_key(&k) {
                keys_request.push(k);
                exps_request.push(e.to_exp());
            }
        }
        let values_request: Vec<Value> = if keys_request.is_empty() {
            vec![]
        } else {
            warn!(
                "Some arguments are missing, requesting from vim. Keys: {:?}. Exps: {:?}",
                keys_request,
                exps_request,
            );
            self.eval(&exps_request[..])?
        };
        for (k, v) in keys_request.into_iter().zip(values_request.into_iter()) {
            map.insert(k, v);
        }

        let mut result = vec![];
        for e in exps {
            let k = e.to_string();
            result.push(map.remove(&k).ok_or(format_err!("Failed to get value"))?);
        }

        debug!("End gather_args");
        debug!("{:?} = {:?}", exps, result);
        Ok(serde_json::from_value(Value::Array(result))?)
    }

    fn sync_settings(&self) -> Result<()> {
        let (autoStart, serverCommands, mut selectionUI, trace, settingsPath, loadSettings): (
            u64,
            HashMap<String, Vec<String>>,
            String,
            String,
            String,
            bool,
        ) = self.eval(
            &[
                "!!get(g:, 'LanguageClient_autoStart', v:true)",
                "get(g:, 'LanguageClient_serverCommands', {})",
                "get(g:, 'LanguageClient_selectionUI', '')",
                "get(g:, 'LanguageClient_trace', 'Off')",
                "get(g:, 'LanguageClient_settingsPath', '.vim/settings.json')",
                "get(g:, 'LanguageClient_loadSettings', v:true)",
            ][..],
        )?;
        // vimscript use 1 for true, 0 for false.
        let autoStart = autoStart == 1;

        let trace = match trace.to_uppercase().as_str() {
            "OFF" => TraceOption::Off,
            "Messages" => TraceOption::Messages,
            "Verbose" => TraceOption::Verbose,
            _ => return Err(format_err!("Unknown trace option: {:?}", trace)),
        };

        if selectionUI == "" {
            let loaded_fzf: i32 = self.eval("get(g:, 'loaded_fzf')")?;
            if loaded_fzf == 1 {
                selectionUI = "FZF".into();
            }
        }
        let selectionUI = match selectionUI.to_uppercase().as_str() {
            "FZF" => SelectionUI::FZF,
            "" | "LOCATIONLIST" | "LOCATION-LIST" => SelectionUI::LocationList,
            _ => return Err(format_err!("Unknown selectionUI option: {:?}", selectionUI)),
        };

        let (diagnosticsEnable, diagnosticsList, diagnosticsDisplay): (u64, DiagnosticsList, Value) = self.eval(
            &[
                "!!get(g:, 'LanguageClient_diagnosticsEnable', v:true)",
                "get(g:, 'LanguageClient_diagnosticsList', 'Quickfix')",
                "get(g:, 'LanguageClient_diagnosticsDisplay', v:null)",
            ][..],
        )?;
        let diagnosticsEnable = diagnosticsEnable == 1;

        self.update(|state| {
            state.autoStart = autoStart;
            state.serverCommands.merge(serverCommands);
            state.selectionUI = selectionUI;
            state.trace = trace;
            state.diagnosticsEnable = diagnosticsEnable;
            state.diagnosticsList = diagnosticsList;
            state.diagnosticsDisplay =
                serde_json::from_value(serde_json::to_value(&state.diagnosticsDisplay)?.combine(diagnosticsDisplay))?;
            state.settingsPath = settingsPath;
            state.loadSettings = loadSettings;
            Ok(())
        })?;

        Ok(())
    }

    fn define_signs(&self) -> Result<()> {
        info!("Define signs");
        let cmd = self.get(|state| {
            let mut cmd = "echo".to_owned();

            for entry in state.diagnosticsDisplay.values() {
                cmd += format!(
                    " | execute 'sign define LanguageClient{} text={} texthl={}'",
                    entry.name,
                    entry.signText,
                    entry.signTexthl,
                ).as_str();
            }

            Ok(cmd)
        })?;
        self.command(cmd.as_str())?;
        info!("Define signs");
        Ok(())
    }

    fn apply_WorkspaceEdit(&self, edit: &WorkspaceEdit) -> Result<()> {
        debug!("Begin apply WorkspaceEdit: {:?}", edit);
        let (filename, line, character): (String, u64, u64) = self.gather_args(
            &[
                VimVarName::Filename,
                VimVarName::Line,
                VimVarName::Character,
            ],
            &None,
        )?;
        for (uri, edits) in &edit.changes {
            self.apply_TextEdits(uri.path(), edits.as_slice())?;
        }
        debug!("End apply WorkspaceEdit");
        self.goto_location(&filename, line, character)?;
        self.textDocument_didChange(&None)?;
        Ok(())
    }

    fn apply_TextEdits(&self, filename: &str, edits: &[TextEdit]) -> Result<()> {
        debug!("Begin apply WorkspaceEdit: {:?}", edits);
        let mut edits = edits.to_vec();
        edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        edits.reverse();
        self.goto_location(filename, 0, 0)?;
        let mut lines: Vec<String> = self.getbufline(filename)?;
        lines = apply_TextEdits(&lines, &edits)?;
        let fixendofline: u64 = self.eval("&fixendofline")?;
        if fixendofline == 1 && lines[lines.len() - 1].is_empty() {
            lines.pop();
        }
        self.notify(None, "setline", json!([1, lines]))?;
        debug!("End apply WorkspaceEdit");
        Ok(())
    }

    fn display_diagnostics(&self, filename: &str, diagnostics: &[Diagnostic]) -> Result<()> {
        // Signs.
        let mut signs: Vec<_> = diagnostics
            .iter()
            .map(|dn| {
                let severity = dn.severity.unwrap_or(DiagnosticSeverity::Information);
                Sign::new(dn.range.start.line + 1, severity)
            })
            .collect();
        signs.sort();
        signs.dedup();

        let cmd = self.update(|state| {
            let signs_prev = state
                .signs
                .insert(filename.to_owned(), signs.clone())
                .unwrap_or(vec![]);
            Ok(get_command_update_signs(&signs_prev, &signs, filename))
        })?;
        self.command(cmd.as_str())?;

        // Quickfix.
        let qflist: Vec<_> = diagnostics
            .iter()
            .map(|dn| {
                QuickfixEntry {
                    filename: filename.to_owned(),
                    lnum: dn.range.start.line + 1,
                    col: Some(dn.range.start.character + 1),
                    nr: dn.code.clone().map(|ns| ns.to_string()),
                    text: Some(dn.message.to_owned()),
                    typee: dn.severity.map(|sev| sev.to_quickfix_entry_type()),
                }
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
                0,
                0,
                "''",
                1,
                1,
                1
            );
            source = Some(self.eval(exp)?);
            self.update(|state| {
                state.highlight_source = source;
                Ok(())
            })?;
        }
        let source = source.ok_or(format_err!("Failed to get highlight source id"))?;
        let diagnosticsDisplay = self.get(|state| Ok(state.diagnosticsDisplay.clone()))?;

        // Optimize.
        self.call(None, "nvim_buf_clear_highlight", json!([0, source, 1, -1]))?;
        for dn in diagnostics.iter() {
            let severity = dn.severity.unwrap_or(DiagnosticSeverity::Information);
            let hl_group = diagnosticsDisplay
                .get(&severity.to_int()?)
                .ok_or(format_err!("Failed to get display"))?
                .texthl
                .clone();
            self.notify(
                None,
                "nvim_buf_add_highlight",
                json!([
                    0,
                    source,
                    hl_group,
                    dn.range.start.line + 1,
                    dn.range.start.character + 1,
                    dn.range.end.character + 1
                ]),
            )?;
        }

        Ok(())
    }

    /// Sample RPC method.
    fn hello(&self, params: &Option<Params>) -> Result<Value> {
        info!("Received params: {:?}", params);
        let character: u64 = self.eval(VimVarName::Character)?;
        info!("character = {}", character);

        Ok(json!("world"))
    }

    fn languageClient_getState(&self, _params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__GetState);
        let s = self.get(|state| Ok(serde_json::to_string(state)?))?;
        info!("End {}", REQUEST__GetState);
        Ok(Value::String(s))
    }

    fn languageClient_isAlive(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__IsAlive);
        let (languageId,): (String,) = self.gather_args(&[VimVarName::LanguageId], params)?;
        let is_alive = self.get(|state| Ok(state.writers.contains_key(&languageId)))?;
        info!("End {}", REQUEST__IsAlive);
        Ok(Value::Bool(is_alive))
    }

    fn languageClient_startServer(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__StartServer);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
            ],
            params,
        )?;

        if !buftype.is_empty() || filename.is_empty() {
            return Ok(Value::Null);
        }

        if self.get(|state| Ok(state.writers.contains_key(&languageId)))? {
            format_err!(
                "Language client has already started for language {}.",
                &languageId
            );
        }

        self.sync_settings()?;

        let mut command = self.get(|state| {
            state
                .serverCommands
                .get(&languageId)
                .cloned()
                .ok_or(format_err!(
                    "No language server command found for type: {}.",
                    &languageId
                ))
        })?;

        if command[0].starts_with('~') {
            let home = env::home_dir().ok_or(format_err!("Failed to get home dir"))?;
            let home = home.to_str()
                .ok_or(format_err!("Failed to convert PathBuf to str"))?;
            command[0] = command[0].replacen('~', home, 1);
        }

        let server_logpath = get_server_logpath();
        let stderr = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&server_logpath)?;

        let process = std::process::Command::new(command
            .get(0)
            .ok_or(format_err!("Failed to get command[0]"))?)
            .args(&command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()?;

        let reader = BufReader::new(process
            .stdout
            .ok_or(format_err!("Failed to get subprocess stdout"))?);
        let writer = BufWriter::new(process
            .stdin
            .ok_or(format_err!("Failed to get subprocess stdin"))?);

        self.update(|state| {
            state.writers.insert(languageId.clone(), writer);
            Ok(())
        })?;

        let state = Arc::clone(self);
        let languageId_clone = languageId.clone();
        std::thread::Builder::new()
            .name(format!("RPC-{}", languageId))
            .spawn(move || {
                if let Err(err) = state.loop_message(reader, Some(languageId_clone)) {
                    error!("Thread error out: {}", err);
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
        let params = params.clone().ok_or(format_err!("Empty params"))?;
        let map = match params {
            Params::Map(map) => Value::Object(map),
            _ => return Err(format_err!("Unexpected params type!")),
        };
        let map = serde_json::from_value(map)?;
        self.update(|state| Ok(state.serverCommands.merge(map)))?;
        let exp = format!(
            "let g:LanguageClient_serverCommands={}",
            serde_json::to_string(&self.get(|state| Ok(state.serverCommands.clone()))?)?
        );
        self.command(exp.as_str())?;
        info!("End {}", REQUEST__RegisterServerCommands);
        Ok(Value::Null)
    }

    fn languageClient_handleBufReadPost(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__HandleBufReadPost);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
            ],
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
                    .ok_or(format_err!("Failed to get root"))?;
                Ok(filename.starts_with(root))
            })?;
            if !is_in_root {
                return Ok(());
            }

            self.textDocument_didOpen(params)?;

            let diagnostics = self.get(|state| {
                state
                    .diagnostics
                    .get(&filename)
                    .cloned()
                    .ok_or(format_err!("No diagnostics"))
            }).unwrap_or(vec![]);
            self.display_diagnostics(filename.as_str(), &diagnostics)?;
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
        let (filename,): (String,) = self.gather_args(&[VimVarName::Filename], params)?;
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
        let (buftype, filename, line): (String, String, u64) = self.gather_args(
            &[VimVarName::Buftype, VimVarName::Filename, VimVarName::Line],
            params,
        )?;
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
                .get(&(filename, line))
                .cloned()
                .ok_or(format_err!("No line diagnostic message"))
        }).unwrap_or("".to_owned());
        if message == self.get(|state| Ok(state.last_line_diagnostic.clone()))? {
            return Ok(());
        }

        self.update(|state| {
            state.last_line_diagnostic = message.clone();
            Ok(())
        })?;
        self.echo_ellipsis(message.as_str())?;

        info!("End {}", NOTIFICATION__HandleCursorMoved);
        Ok(())
    }

    fn initialize(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__Initialize);
        let (languageId, filename): (String, String) =
            self.gather_args(&[VimVarName::LanguageId, VimVarName::Filename], params)?;
        let root = get_rootPath(Path::new(filename.as_str()), languageId.as_str())?
            .to_str()
            .ok_or(format_err!("Failed to convert &Path to &str"))?
            .to_owned();
        self.update(|state| {
            Ok(state.roots.insert(languageId.clone(), root.clone()))
        })?;

        use std::fs::File;

        let settings = || -> Result<Value> {
            let mut f = File::open(Path::new(root.as_str()).join(self.get(|state| Ok(state.settingsPath.clone()))?))?;
            let mut buffer = String::new();
            f.read_to_string(&mut buffer)?;
            Ok(serde_json::from_str(buffer.as_str())?)
        }()
            .unwrap_or(json!({}));
        let initialization_options = Some(settings["initializationOptions"].clone());

        let result = self.call(
            Some(languageId.as_str()),
            REQUEST__Initialize,
            InitializeParams {
                process_id: Some(std::process::id().into()),
                root_path: Some(root.clone()),
                root_uri: Some(root.as_str().to_url()?),
                initialization_options,
                capabilities: ClientCapabilities {
                    workspace: None,
                    text_document: None,
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

        info!("End {}", REQUEST__Initialize);
        self.registerCMSource(languageId.as_str(), &result)?;
        Ok(result)
    }

    fn registerCMSource(&self, languageId: &str, result: &Value) -> Result<()> {
        info!("Begin register NCM source");
        let exists_CMRegister: u64 = self.eval("exists('cm#register')")?;
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
                    .map(|c| regex::escape(c.as_str()))
                    .collect();
                strings
            })
            .unwrap_or(vec![]);

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

    fn get_line(&self, filename: &str, line: u64) -> Result<String> {
        let value = self.call(None, "getbufline", json!([filename, line + 1]))?;
        let mut texts: Vec<String> = serde_json::from_value(value)?;
        let mut text = texts.pop().unwrap_or("".to_owned());

        if text.is_empty() {
            let reader = BufReader::new(File::open(filename)?);
            text = reader
                .lines()
                .nth(line.to_usize()?)
                .ok_or(format_err!("Failed to get line"))??;
        }

        Ok(text.as_str().strip().to_owned())
    }

    fn NCM_refresh(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__NCMRefresh);
        let params = match *params {
            None | Some(Params::None) => return Err(format_err!("Empty params!")),
            Some(Params::Map(_)) => return Err(format_err!("Expecting array. Got dict.")),
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
        let result: CompletionResult = serde_json::from_value(result)?;
        let is_incomplete = match result {
            CompletionResult::Array(_) => false,
            CompletionResult::Object(ref list) => list.is_incomplete,
        };
        let matches: Vec<VimCompleteItem> = match result {
            CompletionResult::Array(arr) => arr,
            CompletionResult::Object(list) => list.items,
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
        let result: CompletionResult = serde_json::from_value(result)?;
        let matches: Vec<VimCompleteItem> = match result {
            CompletionResult::Array(arr) => arr,
            CompletionResult::Object(list) => list.items,
        }.into_iter()
            .map(|lspitem| lspitem.into())
            .collect();
        info!("End {}", REQUEST__OmniComplete);
        Ok(serde_json::to_value(matches)?)
    }

    fn textDocument_references(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__References);
        self.textDocument_didChange(params)?;

        let (buftype, languageId, filename, line, character, handle): (String, String, String, u64, u64, bool) =
            self.gather_args(
                &[
                    VimVarName::Buftype,
                    VimVarName::LanguageId,
                    VimVarName::Filename,
                    VimVarName::Line,
                    VimVarName::Character,
                    VimVarName::Handle,
                ],
                params,
            )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(languageId.as_str()),
            REQUEST__References,
            ReferenceParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
                position: Position { line, character },
                context: ReferenceContext {
                    include_declaration: true,
                },
            },
        )?;

        if !handle {
            return Ok(result);
        }

        let locations: Vec<Location> = serde_json::from_value(result.clone())?;

        match self.get(|state| Ok(state.selectionUI.clone()))? {
            SelectionUI::FZF => {
                let root = self.get(|state| {
                    state
                        .roots
                        .get(&languageId)
                        .cloned()
                        .ok_or(format_err!("Failed to get root"))
                })?;
                let source: Vec<_> = locations
                    .iter()
                    .map(|loc| {
                        let filename = loc.uri.path();
                        let relpath = diff_paths(Path::new(loc.uri.path()), Path::new(&root))
                            .unwrap_or(Path::new(filename).to_path_buf());
                        let relpath = relpath.to_str().unwrap_or(filename);
                        let start = loc.range.start;
                        let text = self.get_line(filename, start.line).unwrap_or("".to_owned());
                        format!(
                            "{}:{}:{}:\t{}",
                            relpath,
                            start.line + 1,
                            start.character + 1,
                            text
                        )
                    })
                    .collect();

                self.notify(
                    None,
                    "s:FZF",
                    json!([source, format!("s:{}", NOTIFICATION__FZFSinkLocation)]),
                )?;
            }
            SelectionUI::LocationList => {
                let loclist: Vec<_> = locations
                    .iter()
                    .map(|loc| {
                        let filename = loc.uri.path();
                        let start = loc.range.start;
                        let text = self.get_line(filename, start.line).unwrap_or("".to_owned());
                        json!({
                        "filename": filename,
                        "lnum": start.line + 1,
                        "col": start.character + 1,
                        "text": text,
                    })
                    })
                    .collect();

                self.notify(None, "setloclist", json!([0, loclist]))?;
                self.echo("References populated to location list.")?;
            }
        }

        info!("End {}", REQUEST__References);
        Ok(result)
    }

    fn textDocument_didOpen(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__DidOpenTextDocument);
        let (buftype, languageId, filename, text): (String, String, String, Vec<String>) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
                VimVarName::Text,
            ],
            params,
        )?;

        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }

        let text_document = TextDocumentItem {
            uri: filename.as_str().to_url()?,
            language_id: Some(languageId.clone()),
            version: None,
            text: text.join("\n"),
        };

        self.update(|state| {
            Ok(
                state
                    .text_documents
                    .insert(filename.clone(), text_document.clone()),
            )
        })?;

        self.notify(
            Some(languageId.as_str()),
            NOTIFICATION__DidOpenTextDocument,
            DidOpenTextDocumentParams { text_document },
        )?;

        self.command("set omnifunc=LanguageClient#complete")?;

        info!("End {}", NOTIFICATION__DidOpenTextDocument);
        Ok(())
    }

    fn textDocument_didChange(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__DidChangeTextDocument);
        let (buftype, languageId, filename, text): (String, String, String, Vec<String>) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
                VimVarName::Text,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }
        if !self.get(|state| Ok(state.text_documents.contains_key(&filename)))? {
            return self.textDocument_didOpen(params);
        }

        let text = text.join("\n");
        let text_state = self.get(|state| {
            state
                .text_documents
                .get(&filename)
                .ok_or(format_err!("No TextDocumentItem"))
                .map(|doc| doc.text.clone())
        }).unwrap_or("".to_owned());
        if text == text_state {
            return Ok(());
        }

        let version = self.update(|state| {
            let document = state
                .text_documents
                .get(&filename)
                .ok_or(format_err!("Failed to get TextDocumentItem"))?
                .clone();

            let document = TextDocumentItem {
                version: document.version.map(|v| v + 1),
                ..document
            };
            state
                .text_documents
                .insert(languageId.clone(), document.clone());
            Ok(document.version.unwrap_or(1))
        })?;

        self.notify(
            Some(languageId.as_str()),
            NOTIFICATION__DidChangeTextDocument,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                    version,
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

        info!("End {}", NOTIFICATION__DidChangeTextDocument);
        Ok(())
    }

    fn textDocument_didSave(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__DidSaveTextDocument);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }

        self.notify(
            Some(languageId.as_str()),
            NOTIFICATION__DidSaveTextDocument,
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
            },
        )?;

        info!("End {}", NOTIFICATION__DidSaveTextDocument);
        Ok(())
    }

    fn textDocument_didClose(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__DidCloseTextDocument);
        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(());
        }

        self.notify(
            Some(languageId.as_str()),
            NOTIFICATION__DidCloseTextDocument,
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
            },
        )?;
        info!("End {}", NOTIFICATION__DidCloseTextDocument);
        Ok(())
    }

    fn textDocument_publishDiagnostics(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__PublishDiagnostics);
        let params: PublishDiagnosticsParams = serde_json::from_value(params.clone().to_value())?;
        if !self.get(|state| Ok(state.diagnosticsEnable))? {
            return Ok(());
        }

        let filename = params.uri.path().to_owned();
        self.update(|state| {
            state
                .diagnostics
                .insert(filename.clone(), params.diagnostics.clone());
            state.line_diagnostics.retain(|fl, _| fl.0 != filename);
            Ok(())
        })?;

        for entry in &params.diagnostics {
            let line = entry.range.start.line;
            let mut msg = String::new();
            if let Some(severity) = entry.severity {
                msg += format!("[{:?}]", severity).as_str();
            }
            if let Some(ref code) = entry.code {
                let s = code.to_string();
                if !s.is_empty() {
                    msg += format!("[{}]", s).as_str();
                }
            }
            msg += entry.message.as_str();
            self.update(|state| {
                state.line_diagnostics.insert((filename.clone(), line), msg);
                Ok(())
            })?;
        }

        info!("End {}", NOTIFICATION__PublishDiagnostics);

        let current_filename: String = self.eval(VimVarName::Filename)?;
        if filename != current_filename {
            return Ok(());
        }

        self.display_diagnostics(filename.as_str(), &params.diagnostics)?;
        self.languageClient_handleCursorMoved(&None)?;

        Ok(())
    }

    fn textDocument_hover(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__Hover);
        let (languageId, filename, line, character): (String, String, u64, u64) = self.gather_args(
            &[
                VimVarName::LanguageId,
                VimVarName::Filename,
                VimVarName::Line,
                VimVarName::Character,
            ],
            params,
        )?;

        let result = self.call(
            Some(languageId.as_str()),
            REQUEST__Hover,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
                position: Position { line, character },
            },
        )?;

        let hover: Hover = serde_json::from_value(result.clone())?;

        let message = hover.to_string();
        self.echomsg(message.as_str())?;

        info!("End {}", REQUEST__Hover);
        Ok(result)
    }

    fn textDocument_definition(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__GotoDefinition);
        let (buftype, languageId, filename, line, character): (String, String, String, u64, u64) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
                VimVarName::Line,
                VimVarName::Character,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        self.textDocument_didChange(params)?;

        let value = self.call(
            Some(languageId.as_str()),
            REQUEST__GotoDefinition,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
                position: Position { line, character },
            },
        )?;

        let response: GotoDefinitionResponse = serde_json::from_value(value.clone())?;

        match response {
            GotoDefinitionResponse::None => {
                self.echowarn("Not found!")?;
                return Ok(Value::Null);
            }
            GotoDefinitionResponse::Scalar(loc) => {
                self.goto_location(
                    loc.uri.path(),
                    loc.range.start.line,
                    loc.range.start.character,
                )?;
            }
            GotoDefinitionResponse::Array(arr) => {
                let loc = arr.get(0).ok_or(format_err!("Not found!"))?;
                self.goto_location(
                    loc.uri.path(),
                    loc.range.start.line,
                    loc.range.start.character,
                )?;
                if arr.len() > 1 {
                    let message = "Multiple defintions found. Goto first!";
                    warn!("{}", message);
                    self.echomsg(message)?;
                }
            }
        };

        info!("End {}", REQUEST__GotoDefinition);
        Ok(value)
    }

    fn textDocument_rename(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__Rename);
        let (buftype, languageId, filename, line, character, cword, new_name): (
            String,
            String,
            String,
            u64,
            u64,
            String,
            Option<String>,
        ) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
                VimVarName::Line,
                VimVarName::Character,
                VimVarName::Cword,
                VimVarName::NewName,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let mut new_name = new_name.unwrap_or("".to_owned());
        if new_name.is_empty() {
            let value = self.call(None, "s:getInput", ["Rename to: ".to_owned(), cword])?;
            new_name = serde_json::from_value(value)?;
        }
        if new_name.is_empty() {
            return Ok(Value::Null);
        }

        let value = self.call(
            Some(languageId.as_str()),
            REQUEST__Rename,
            RenameParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
                position: Position { line, character },
                new_name,
            },
        )?;

        if value == Value::Null {
            return Ok(value);
        }

        let edit: WorkspaceEdit = serde_json::from_value(value.clone())?;
        self.apply_WorkspaceEdit(&edit)?;

        info!("End {}", REQUEST__Rename);
        Ok(value)
    }

    fn textDocument_documentSymbol(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__DocumentSymbols);
        self.textDocument_didChange(params)?;

        let (buftype, languageId, filename): (String, String, String) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
            ],
            params,
        )?;

        if !buftype.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(&languageId),
            REQUEST__DocumentSymbols,
            DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
            },
        )?;

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

        info!("End {}", REQUEST__DocumentSymbols);
        Ok(result)
    }

    fn workspace_symbol(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__WorkspaceSymbols);
        let (buftype, languageId): (String, String) =
            self.gather_args(&[VimVarName::Buftype, VimVarName::LanguageId], params)?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        self.textDocument_didChange(params)?;
        let query = "".to_owned();
        let result = self.call(
            Some(languageId.as_str()),
            REQUEST__WorkspaceSymbols,
            WorkspaceSymbolParams { query },
        )?;

        let symbols: Vec<SymbolInformation> = serde_json::from_value(result.clone())?;

        match self.get(|state| Ok(state.selectionUI.clone()))? {
            SelectionUI::FZF => {
                let root = self.get(|state| {
                    state
                        .roots
                        .get(&languageId)
                        .cloned()
                        .ok_or(format_err!("Failed to get root"))
                })?;
                let source: Vec<_> = symbols
                    .iter()
                    .map(|sym| {
                        let filename = sym.location.uri.path();
                        let relpath = diff_paths(Path::new(sym.location.uri.path()), Path::new(&root))
                            .unwrap_or(Path::new(filename).to_path_buf());
                        let relpath = relpath.to_str().unwrap_or(filename);
                        let start = sym.location.range.start;
                        format!(
                            "{}:{}:{}:\t{}",
                            relpath,
                            start.line + 1,
                            start.character + 1,
                            sym.name
                        )
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
                        "filename": sym.location.uri.path(),
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

        info!("End {}", REQUEST__WorkspaceSymbols);
        Ok(result)
    }

    fn languageClient_FZFSinkLocation(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__FZFSinkLocation);
        let params = match *params {
            None | Some(Params::None) | Some(Params::Map(_)) => {
                return Err(format_err!("Expecting array params!"));
            }
            Some(Params::Array(ref arr)) => Value::Array(arr.clone()),
        };

        let lines: Vec<String> = serde_json::from_value(params)?;
        if lines.is_empty() {
            format_err!("No selection!");
        }
        let mut tokens: Vec<&str> = lines
            .get(0)
            .ok_or(format_err!("Failed to get line"))?
            .split(':')
            .collect();
        tokens.reverse();
        let filename: String = if tokens.len() > 3 {
            let relpath = tokens
                .pop()
                .ok_or(format_err!("Failed to get filepath token"))?
                .to_owned();
            let languageId: String = self.eval(VimVarName::LanguageId)?;
            let root = self.get(|state| {
                state
                    .roots
                    .get(&languageId)
                    .cloned()
                    .ok_or(format_err!("Failed to get root"))
            })?;
            Path::new(&root)
                .join(relpath)
                .to_str()
                .ok_or(format_err!("Failed to convert PathBuf to str"))?
                .to_owned()
        } else {
            self.eval(VimVarName::Filename)?
        };
        let line = tokens
            .pop()
            .ok_or(format_err!("Failed to get line token"))?
            .to_int()? - 1;
        let character = tokens
            .pop()
            .ok_or(format_err!("Failed to get character token"))?
            .to_int()? - 1;

        self.goto_location(filename.as_str(), line, character)?;

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
            .ok_or(format_err!("Failed to get command token"))?;
        let title = tokens
            .get(1)
            .cloned()
            .ok_or(format_err!("Failed to get title token"))?;
        let entry = self.get(|state| {
            state
                .stashed_codeAction_commands
                .iter()
                .find(|e| e.command == command && e.title == title)
                .cloned()
                .ok_or(format_err!("No project root found!"))
        })?;

        //TODO: Try handle client commands.
        // java.apply.workspaceEdit

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

    fn textDocument_codeAction(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__CodeAction);
        let (buftype, languageId, filename, line, character): (String, String, String, u64, u64) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
                VimVarName::Line,
                VimVarName::Character,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let diagnostics: Vec<_> = self.get(|state| {
            Ok(
                state
                    .diagnostics
                    .get(&filename)
                    .ok_or(format_err!("No diagnostics found!"))?
                    .iter()
                    .filter(|dn| {
                        let start = dn.range.start;
                        let end = dn.range.end;
                        start.line <= line && start.character <= character && end.line >= line
                            && end.character >= character
                    })
                    .cloned()
                    .collect(),
            )
        })?;
        let result = self.call(
            Some(languageId.as_str()),
            REQUEST__CodeAction,
            CodeActionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
                //TODO: is this correct?
                range: diagnostics
                    .get(0)
                    .ok_or(format_err!("No diagnostics found!"))?
                    .range,
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

        self.notify(
            None,
            "s:FZF",
            json!([source, format!("s:{}", NOTIFICATION__FZFSinkCommand)]),
        )?;

        info!("End {}", REQUEST__CodeAction);
        Ok(result)
    }

    fn textDocument_completion(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__Completion);

        let (buftype, languageId, filename, line, character): (String, String, String, u64, u64) = self.gather_args(
            &[
                VimVarName::Buftype,
                VimVarName::LanguageId,
                VimVarName::Filename,
                VimVarName::Line,
                VimVarName::Character,
            ],
            params,
        )?;
        if !buftype.is_empty() || languageId.is_empty() {
            return Ok(Value::Null);
        }

        let result = self.call(
            Some(languageId.as_str()),
            REQUEST__Completion,
            TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.as_str().to_url()?,
                },
                position: Position { line, character },
            },
        )?;

        info!("End {}", REQUEST__Completion);
        Ok(result)
    }

    fn workspace_executeCommand(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__ExecuteCommand);
        let (languageId,): (String,) = self.gather_args(&[VimVarName::LanguageId], params)?;
        let (command, arguments): (String, Vec<Value>) = self.gather_args(&["command", "arguments"], params)?;

        let result = self.call(
            Some(languageId.as_str()),
            REQUEST__ExecuteCommand,
            ExecuteCommandParams { command, arguments },
        )?;
        info!("End {}", REQUEST__ExecuteCommand);
        Ok(result)
    }

    fn workspace_applyEdit(&self, params: &Option<Params>) -> Result<Value> {
        info!("Begin {}", REQUEST__ApplyEdit);

        let params: ApplyWorkspaceEditParams = serde_json::from_value(params.clone().to_value())?;
        self.apply_WorkspaceEdit(&params.edit)?;

        info!("End {}", REQUEST__ApplyEdit);

        Ok(serde_json::to_value(
            ApplyWorkspaceEditResponse { applied: true },
        )?)
    }

    fn exit(&self, params: &Option<Params>) -> Result<()> {
        info!("Begin {}", NOTIFICATION__Exit);
        let (languageId,): (String,) = self.gather_args(&[VimVarName::LanguageId], params)?;

        self.notify(Some(&languageId), NOTIFICATION__Exit, Value::Null)?;

        self.update(|state| {
            state.last_cursor_line = 99;
            Ok(())
        })?;

        let signsmap = self.update(|state| {
            state.writers.remove(&languageId);
            let root = state
                .roots
                .remove(&languageId)
                .ok_or(format_err!("No project root found!"))?;

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
            let cmd = get_command_update_signs(&signs, &[], &filename);
            self.command(cmd.as_str())?;
        }

        if self.eval::<_, u64>("exists('#User#LanguageClientStopped')")? == 1 {
            self.command("doautocmd User LanguageClientStopped")?;
        }
        info!("End {}", NOTIFICATION__Exit);
        Ok(())
    }

    // TODO:
    // - formatting
    // - rust extensions
    // - denite integrations
}
