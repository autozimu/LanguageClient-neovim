use super::*;

impl State {
    fn poll_call(&mut self) -> Result<Call> {
        if let Some(msg) = self.pending_calls.pop_front() {
            return Ok(msg);
        }

        loop {
            let msg = self.rx.recv()?;
            match msg {
                Message::MethodCall(lang_id, method_call) => {
                    return Ok(Call::MethodCall(lang_id, method_call));
                }
                Message::Notification(lang_id, notification) => {
                    return Ok(Call::Notification(lang_id, notification));
                }
                Message::Output(output) => {
                    let mid = output.id().to_int()?;
                    self.pending_outputs.insert(mid, output);
                }
            }
        }
    }

    fn poll_output(&mut self, id: Id) -> Result<rpc::Output> {
        if let Some(output) = self.pending_outputs.remove(&id) {
            return Ok(output);
        }

        loop {
            let msg = self.rx.recv_timeout(self.wait_output_timeout)?;
            match msg {
                Message::MethodCall(lang_id, method_call) => self.pending_calls
                    .push_back(Call::MethodCall(lang_id, method_call)),
                Message::Notification(lang_id, notification) => self.pending_calls
                    .push_back(Call::Notification(lang_id, notification)),
                Message::Output(output) => {
                    let mid = output.id().to_int()?;
                    if mid == id {
                        return Ok(output);
                    } else {
                        self.pending_outputs.insert(mid, output);
                    }
                }
            }
        }
    }

    pub fn loop_message(&mut self) -> Result<()> {
        loop {
            match self.poll_call()? {
                Call::MethodCall(lang_id, method_call) => {
                    let result = self.handle_method_call(lang_id.as_deref(), &method_call);
                    if let Err(ref err) = result {
                        if err.downcast_ref::<LCError>().is_none() {
                            error!(
                                "Error handling message: {}\n\nMessage: {}\n\nError: {:?}",
                                err,
                                serde_json::to_string(&method_call).unwrap_or_default(),
                                err
                            );
                        }
                    }
                    let _ = self.output(lang_id.as_deref(), method_call.id, result);
                }
                Call::Notification(lang_id, notification) => {
                    let result = self.handle_notification(lang_id.as_deref(), &notification);
                    if let Err(ref err) = result {
                        if err.downcast_ref::<LCError>().is_none() {
                            error!(
                                "Error handling message: {}\n\nMessage: {}\n\nError: {:?}",
                                err,
                                serde_json::to_string(&notification).unwrap_or_default(),
                                err
                            );
                        }
                    }
                }
            }

            if let Err(err) = self.check_fs_notify() {
                warn!("{:?}", err);
            }
        }
    }

    /// Send message to RPC server.
    fn write(&mut self, languageId: Option<&str>, message: &str) -> Result<()> {
        info!("=> {:?} {}", languageId, message);
        if let Some(languageId) = languageId {
            let writer = self.writers
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
        } else {
            println!("Content-Length: {}\n\n{}", message.len(), message);
        }

        Ok(())
    }

    /// RPC method call.
    pub fn call<P, V>(&mut self, languageId: Option<&str>, method: &str, params: P) -> Result<V>
    where
        P: Serialize,
        V: DeserializeOwned,
    {
        self.id += 1;
        let id = self.id;

        let method_call = rpc::MethodCall {
            jsonrpc: Some(rpc::Version::V2),
            id: rpc::Id::Num(id),
            method: method.into(),
            params: params.to_params()?,
        };

        let message = serde_json::to_string(&method_call)?;
        self.write(languageId, &message)?;

        match self.poll_output(id)? {
            rpc::Output::Success(success) => Ok(serde_json::from_value(success.result)?),
            rpc::Output::Failure(failure) => Err(format_err!("{}", failure.error.message)),
        }
    }

    /// RPC notification.
    pub fn notify<P>(&mut self, languageId: Option<&str>, method: &str, params: P) -> Result<()>
    where
        P: Serialize,
    {
        let notification = rpc::Notification {
            jsonrpc: Some(rpc::Version::V2),
            method: method.to_owned(),
            params: params.to_params()?,
        };

        let message = serde_json::to_string(&notification)?;
        self.write(languageId, &message)?;

        Ok(())
    }

    /// Write an RPC call output.
    fn output(
        &mut self,
        languageId: Option<&str>,
        id: rpc::Id,
        result: Result<Value>,
    ) -> Result<()> {
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
        self.write(languageId, &message)?;
        Ok(())
    }

    /////// Vim wrappers ///////

    #[allow(needless_pass_by_value)]
    pub fn eval<E, T>(&mut self, exp: E) -> Result<T>
    where
        E: VimExp,
        T: DeserializeOwned,
    {
        let result = self.call(None, "eval", exp.to_exp())?;
        Ok(serde_json::from_value(result)?)
    }

    pub fn command<S: AsRef<str>>(&mut self, cmd: S) -> Result<()> {
        self.call::<_, u8>(None, "execute", cmd.as_ref())?;
        Ok(())
    }

    ////// Vim builtin function wrappers ///////

    pub fn echo<S>(&mut self, message: S) -> Result<()>
    where
        S: AsRef<str> + Serialize,
    {
        if self.call::<_, u8>(None, "s:Echo", message)? != 0 {
            bail!("return value is failure");
        }
        Ok(())
    }

    pub fn echo_ellipsis<S: AsRef<str>>(&mut self, message: S) -> Result<()> {
        let message = message.as_ref().lines().collect::<Vec<_>>().join(" ");
        self.call::<_, u8>(None, "s:EchoEllipsis", message)?;
        Ok(())
    }

    pub fn echomsg<S>(&mut self, message: S) -> Result<()>
    where
        S: AsRef<str> + Serialize,
    {
        if self.call::<_, u8>(None, "s:Echomsg", message)? != 0 {
            bail!("Failed to echomsg!");
        }
        Ok(())
    }

    pub fn echoerr<S>(&mut self, message: S) -> Result<()>
    where
        S: AsRef<str> + Serialize,
    {
        if self.call::<_, u8>(None, "s:Echoerr", message)? != 0 {
            bail!("Failed to echo error!");
        }
        Ok(())
    }

    pub fn echowarn<S>(&mut self, message: S) -> Result<()>
    where
        S: AsRef<str> + Serialize,
    {
        if self.call::<_, u8>(None, "s:Echowarn", message)? != 0 {
            bail!("Failed to echo warning!");
        }
        Ok(())
    }

    pub fn cursor(&mut self, lnum: u64, col: u64) -> Result<()> {
        if self.call::<_, u8>(None, "cursor", json!([lnum, col]))? != 0 {
            bail!("Failed to set cursor!");
        }
        Ok(())
    }

    pub fn setline(&mut self, lnum: u64, text: &[String]) -> Result<()> {
        if self.call::<_, u8>(None, "setline", json!([lnum, text]))? != 0 {
            bail!("Failed to set buffer content!");
        }
        Ok(())
    }

    pub fn goto_location<P: AsRef<Path>>(
        &mut self,
        goto_cmd: &Option<String>,
        path: P,
        line: u64,
        character: u64,
    ) -> Result<()> {
        let path = path.as_ref().to_string_lossy();

        if path.starts_with("jdt://") {
            return self.goto_location_jdt(goto_cmd, &path, line, character);
        }

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

    pub fn setqflist(&mut self, list: &[QuickfixEntry]) -> Result<()> {
        if self.call::<_, u8>(None, "setqflist", json!([list, "r"]))? != 0 {
            bail!("Failed to set quickfix list!");
        }
        Ok(())
    }

    pub fn setloclist(&mut self, list: &[QuickfixEntry]) -> Result<()> {
        if self.call::<_, u8>(None, "setloclist", json!([0, list, "r"]))? != 0 {
            bail!("Failed to set location list!");
        }
        Ok(())
    }

    pub fn get<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&State) -> Result<T>,
    {
        f(self)
    }

    pub fn update<F, T>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&mut State) -> Result<T>,
    {
        use log::Level;

        let before = if log_enabled!(Level::Debug) {
            let s = serde_json::to_string(self)?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };
        let result = f(self);
        let after = if log_enabled!(Level::Debug) {
            let s = serde_json::to_string(self)?;
            serde_json::from_str(&s)?
        } else {
            Value::default()
        };
        for (k, (v1, v2)) in diff_value(&before, &after, "state") {
            debug!("{}: {} ==> {}", k, v1, v2);
        }
        result
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum RawMessage {
    Notification(rpc::Notification),
    MethodCall(rpc::MethodCall),
    Output(rpc::Output),
}

pub fn loop_reader<T: BufRead>(
    input: T,
    languageId: &Option<String>,
    tx: &Sender<Message>,
) -> Result<()> {
    // Count how many consequent empty lines.
    let mut count_empty_lines = 0;

    let mut input = input;
    let mut content_length = 0;
    loop {
        let mut message = String::new();
        let mut line = String::new();
        if languageId.is_some() {
            input.read_line(&mut line)?;
            let line = line.trim();
            if line.is_empty() {
                count_empty_lines += 1;
                if count_empty_lines > 5 {
                    let mut message = "".to_string();
                    if let Ok(log_server) = read_to_string(get_logpath_server()) {
                        message += "\n\nlanguage server stderr:\n";
                        message += &log_server;
                    }

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
        info!("<= {:?} {}", languageId, message);
        // FIXME: Remove extra `meta` property from javascript-typescript-langserver.
        let s = message.replace(r#","meta":{}"#, "");
        let message = serde_json::from_str(&s);
        if let Err(ref err) = message {
            error!(
                "Failed to deserialize output: {}\n\n Message: {}\n\nError: {:?}",
                err, s, err
            );
            continue;
        }
        // TODO: cleanup.
        let message = message.unwrap();
        let message = match message {
            RawMessage::MethodCall(method_call) => {
                Message::MethodCall(languageId.clone(), method_call)
            }
            RawMessage::Notification(notification) => {
                Message::Notification(languageId.clone(), notification)
            }
            RawMessage::Output(output) => Message::Output(output),
        };
        tx.send(message)?;
    }

    Ok(())
}
