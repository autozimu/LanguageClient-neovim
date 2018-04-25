use super::*;

pub trait IVim {
    fn get<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&State) -> Result<T>;
    fn update<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut State) -> Result<T>;
    fn loop_message<T: BufRead>(&self, input: T, languageId: Option<String>) -> Result<()>;

    /// Handle an incoming message.
    fn handle_message<H: IRpcHandler>(
        &self,
        handler: &H,
        languageId: Option<&str>,
        message: &str,
    ) -> Result<()> {
        if let Ok(output) = serde_json::from_str::<rpc::Output>(message) {
            let tx = self.update(|state| {
                state
                    .txs
                    .remove(&output.id().to_int()?)
                    .ok_or_else(|| {
                        format_err!("Failed to get channel sender! id: {:?}", output.id())
                    })?
                    .into_inner()
                    .map_err(|e| format_err!("{:?}", e))
            })?;
            let result = match output {
                rpc::Output::Success(success) => Ok(success.result),
                rpc::Output::Failure(failure) => Err(format_err!("{}", failure.error.message)),
            };
            tx.send(result)?;
            return Ok(());
        }

        // FIXME
        let message = message.replace(r#","meta":{}"#, "");

        let call = serde_json::from_str(&message)?;
        match call {
            rpc::Call::MethodCall(method_call) => {
                let result = handler.handle_request(languageId, &method_call);
                if let Err(ref err) = result {
                    if err.downcast_ref::<LCError>().is_none() {
                        error!(
                            "Error handling message: {}\nMessage: {}\nError: {:?}",
                            err, message, err
                        );
                    }
                }
                self.output(languageId, method_call.id, result)
            }
            rpc::Call::Notification(notification) => {
                handler.handle_notification(languageId, &notification)
            }
            rpc::Call::Invalid(id) => bail!("Invalid message of id: {:?}", id),
        }
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
                writer.flush()?;
                Ok(())
            })?;
        } else {
            println!("Content-Length: {}\n\n{}", message.len(), message);
        }

        Ok(())
    }

    /// Write an RPC call output.
    fn output(&self, languageId: Option<&str>, id: rpc::Id, result: Result<Value>) -> Result<()> {
        let response = match result {
            Ok(ok) => rpc::Output::Success(rpc::Success {
                jsonrpc: Some(rpc::Version::V2),
                id,
                result: ok,
            }),
            Err(err) => rpc::Output::Failure(rpc::Failure {
                jsonrpc: Some(rpc::Version::V2),
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
    fn call<P: Serialize, V: DeserializeOwned>(
        &self,
        languageId: Option<&str>,
        method: &str,
        params: P,
    ) -> Result<V> {
        let id = self.update(|state| {
            state.id += 1;
            Ok(state.id)
        })?;

        let method_call = rpc::MethodCall {
            jsonrpc: Some(rpc::Version::V2),
            id: rpc::Id::Num(id),
            method: method.into(),
            params: params.to_params()?,
        };

        let (tx, cx) = channel();
        self.update(|state| {
            state.txs.insert(id, Mutex::new(tx));
            Ok(())
        })?;

        let message = serde_json::to_string(&method_call)?;
        info!("=> {}", message);
        self.write(languageId, &message)?;

        let value = cx.recv_timeout(std::time::Duration::from_secs(60 * 5))??;
        Ok(serde_json::from_value(value)?)
    }

    /// RPC notification.
    fn notify<P: Serialize>(
        &self,
        languageId: Option<&str>,
        method: &str,
        params: P,
    ) -> Result<()> {
        let notification = rpc::Notification {
            jsonrpc: Some(rpc::Version::V2),
            method: method.to_owned(),
            params: params.to_params()?,
        };

        let message = serde_json::to_string(&notification)?;
        info!("=> {}", message);
        self.write(languageId, &message)?;

        Ok(())
    }

    fn eval<E, T>(&self, exp: E) -> Result<T>
    where
        E: VimExp,
        T: DeserializeOwned,
    {
        let result = self.call(None, "eval", exp.to_exp())?;
        Ok(serde_json::from_value(result)?)
    }

    fn command<S: AsRef<str>>(&self, cmd: S) -> Result<()> {
        self.call::<_, u8>(None, "execute", cmd.as_ref())?;
        Ok(())
    }

    ////// Vim builtin function wrappers ///////

    fn echo<S: AsRef<str>>(&self, message: S) -> Result<()> {
        let message = escape_single_quote(message.as_ref());
        let cmd = format!("echo '{}'", message);
        self.command(cmd)
    }

    fn echo_ellipsis<S: AsRef<str>>(&self, message: S) -> Result<()> {
        let message = message.as_ref().lines().collect::<Vec<_>>().join(" ");
        self.call::<_, u8>(None, "s:EchoEllipsis", message)?;
        Ok(())
    }

    fn echomsg<S: AsRef<str>>(&self, message: S) -> Result<()> {
        let message = escape_single_quote(message);
        let cmd = format!("echomsg '{}'", message);
        self.command(cmd)
    }

    fn echoerr<S: AsRef<str>>(&self, message: S) -> Result<()> {
        let message = escape_single_quote(message);
        let cmd = format!("echohl Error | echomsg '{}' | echohl None", message);
        self.command(cmd)
    }

    fn echowarn<S: AsRef<str>>(&self, message: S) -> Result<()> {
        let message = escape_single_quote(message);
        let cmd = format!("echohl WarningMsg | echomsg '{}' | echohl None", message);
        self.command(cmd)
    }

    fn goto_location<P: AsRef<Path>>(
        &self,
        goto_cmd: &Option<String>,
        path: P,
        line: u64,
        character: u64,
    ) -> Result<()> {
        let path = path.as_ref().to_string_lossy();
        let mut cmd = "echo | ".to_string();
        let goto;
        if let Some(ref goto_cmd) = *goto_cmd {
            goto = goto_cmd.as_str();
        } else if self.eval::<_, i64>(format!("bufnr('{}')", path))? == -1 {
            goto = "edit";
        } else {
            cmd += "execute 'normal m`' | ";
            goto = "buffer";
        };
        cmd += &format!(
            "execute 'normal! m`' | execute '{} +:call\\ cursor({},{}) ' . fnameescape('{}')",
            goto,
            line + 1,
            character + 1,
            path
        );

        self.command(cmd)
    }
}

impl IVim for Arc<RwLock<State>> {
    fn get<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&State) -> Result<T>,
    {
        let state = self.read()
            .or_else(|_| Err(err_msg("Failed to lock state")))?;
        f(&state)
    }

    fn update<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut State) -> Result<T>,
    {
        use log::Level;

        let mut state = RwLock::write(self).or_else(|_| Err(err_msg("Failed to lock state")))?;
        let before = if log_enabled!(Level::Debug) {
            let s = serde_json::to_string(&*state)?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };
        let result = f(&mut state);
        let after = if log_enabled!(Level::Debug) {
            let s = serde_json::to_string(&*state)?;
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
                let line = line.trim();
                if line.is_empty() {
                    count_empty_lines += 1;
                    if count_empty_lines > 5 {
                        if let Err(err) = self.cleanup(&languageId) {
                            error!("Error when cleanup: {:?}", err);
                        }

                        let mut message =
                            format!("Language server ({}) exited unexpectedly!", languageId);
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

            let message = message.trim();
            if message.is_empty() {
                continue;
            }
            info!("<= {}", message);
            let state_clone = self.clone();
            let languageId_clone = languageId.clone();
            let message_clone = message.to_string();
            let spawn_result = std::thread::Builder::new()
                .name(format!(
                    "Handler-{}",
                    languageId.as_ref().map_or("main", String::as_str)
                ))
                .spawn(move || {
                    let state = state_clone;
                    let languageId = languageId_clone;
                    let message = message_clone;

                    if let Err(err) = state.handle_message(&state, languageId.as_deref(), &message)
                    {
                        if err.downcast_ref::<LCError>().is_none() {
                            error!(
                                "Error handling message: {}\nMessage: {}\nError: {:?}",
                                err, message, err
                            );
                        }
                    }
                });
            if let Err(err) = spawn_result {
                error!("Failed to spawn handler: {:?}", err);
            }
        }

        if languageId.is_some() {
            loop {
                let event = self.update(|state| {
                    Ok(state
                        .watcher_rx
                        .lock()
                        .map_err(|_| err_msg("Failed to lock watcher_rx"))?
                        .try_recv()?)
                });
                let event = match event {
                    Ok(event) => event,
                    Err(err) => {
                        error!("{}", err);
                        break;
                    }
                };

                warn!("File system event: {:?}", event);
            }
        }

        Ok(())
    }
}
