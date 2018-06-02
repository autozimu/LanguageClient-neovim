use super::*;
use lsp::notification::Notification;
use lsp::request::Request;

impl State {
    pub fn handle_method_call(
        &mut self,
        languageId: Option<&str>,
        method_call: &rpc::MethodCall,
    ) -> Result<Value> {
        let user_handler = self.get(|state| {
            state
                .user_handlers
                .get(&method_call.method)
                .cloned()
                .ok_or_else(|| err_msg("No user handler"))
        });
        if let Ok(user_handler) = user_handler {
            return self.call(None, &user_handler, method_call.params.clone());
        }

        match method_call.method.as_str() {
            lsp::request::RegisterCapability::METHOD => {
                self.client_registerCapability(languageId.unwrap_or_default(), &method_call.params)
            }
            lsp::request::UnregisterCapability::METHOD => self.client_unregisterCapability(
                languageId.unwrap_or_default(),
                &method_call.params,
            ),
            lsp::request::HoverRequest::METHOD => self.textDocument_hover(&method_call.params),
            m @ lsp::request::GotoDefinition::METHOD
            | m @ REQUEST__CqueryBase
            | m @ REQUEST__CqueryCallers
            | m @ REQUEST__CqueryDerived
            | m @ REQUEST__CqueryVars
            | m @ lsp::request::GotoTypeDefinition::METHOD
            | m @ lsp::request::GotoImplementation::METHOD => {
                self.find_locations(m, &method_call.params)
            }
            lsp::request::Rename::METHOD => self.textDocument_rename(&method_call.params),
            lsp::request::DocumentSymbol::METHOD => {
                self.textDocument_documentSymbol(&method_call.params)
            }
            lsp::request::WorkspaceSymbol::METHOD => self.workspace_symbol(&method_call.params),
            lsp::request::CodeActionRequest::METHOD => {
                self.textDocument_codeAction(&method_call.params)
            }
            lsp::request::Completion::METHOD => self.textDocument_completion(&method_call.params),
            lsp::request::SignatureHelpRequest::METHOD => {
                self.textDocument_signatureHelp(&method_call.params)
            }
            lsp::request::References::METHOD => self.textDocument_references(&method_call.params),
            lsp::request::Formatting::METHOD => self.textDocument_formatting(&method_call.params),
            lsp::request::RangeFormatting::METHOD => {
                self.textDocument_rangeFormatting(&method_call.params)
            }
            lsp::request::ResolveCompletionItem::METHOD => {
                self.completionItem_resolve(&method_call.params)
            }
            lsp::request::ExecuteCommand::METHOD => {
                self.workspace_executeCommand(&method_call.params)
            }
            lsp::request::ApplyWorkspaceEdit::METHOD => {
                self.workspace_applyEdit(&method_call.params)
            }
            REQUEST__RustImplementations => self.rustDocument_implementations(&method_call.params),
            // Extensions.
            REQUEST__GetState => self.languageClient_getState(&method_call.params),
            REQUEST__IsAlive => self.languageClient_isAlive(&method_call.params),
            REQUEST__StartServer => self.languageClient_startServer(&method_call.params),
            REQUEST__RegisterServerCommands => {
                self.languageClient_registerServerCommands(&method_call.params)
            }
            REQUEST__SetLoggingLevel => self.languageClient_setLoggingLevel(&method_call.params),
            REQUEST__RegisterHandlers => self.languageClient_registerHandlers(&method_call.params),
            REQUEST__NCMRefresh => self.NCM_refresh(&method_call.params),
            REQUEST__ExplainErrorAtPoint => {
                self.languageClient_explainErrorAtPoint(&method_call.params)
            }
            REQUEST__OmniComplete => self.languageClient_omniComplete(&method_call.params),
            REQUEST__ClassFileContents => self.java_classFileContents(&method_call.params),

            _ => {
                let languageId_target = if languageId.is_some() {
                    // Message from language server. No handler found.
                    let msg = format!("Message not handled: {:?}", method_call);
                    if method_call.method.starts_with('$') {
                        warn!("{}", msg);
                        return Ok(Value::default());
                    } else {
                        return Err(err_msg(msg));
                    }
                } else {
                    // Message from vim. Proxy to language server.
                    let (languageId_target,): (String,) =
                        self.gather_args(&[VimVar::LanguageId], &method_call.params)?;
                    Some(languageId_target)
                };

                self.call(
                    languageId_target.as_deref(),
                    &method_call.method,
                    &method_call.params,
                )
            }
        }
    }

    pub fn handle_notification(
        &mut self,
        languageId: Option<&str>,
        notification: &rpc::Notification,
    ) -> Result<()> {
        let user_handler = self.get(|state| {
            state
                .user_handlers
                .get(&notification.method)
                .cloned()
                .ok_or_else(|| err_msg("No user handler"))
        });
        if let Ok(user_handler) = user_handler {
            self.call::<_, u8>(None, &user_handler, notification.params.clone())?;
            return Ok(());
        }

        match notification.method.as_str() {
            lsp::notification::DidChangeConfiguration::METHOD => {
                self.workspace_didChangeConfiguration(&notification.params)?
            }
            lsp::notification::DidOpenTextDocument::METHOD => {
                self.textDocument_didOpen(&notification.params)?
            }
            lsp::notification::DidChangeTextDocument::METHOD => {
                self.textDocument_didChange(&notification.params)?
            }
            lsp::notification::DidSaveTextDocument::METHOD => {
                self.textDocument_didSave(&notification.params)?
            }
            lsp::notification::DidCloseTextDocument::METHOD => {
                self.textDocument_didClose(&notification.params)?
            }
            lsp::notification::PublishDiagnostics::METHOD => {
                self.textDocument_publishDiagnostics(&notification.params)?
            }
            lsp::notification::LogMessage::METHOD => self.window_logMessage(&notification.params)?,
            lsp::notification::ShowMessage::METHOD => {
                self.window_showMessage(&notification.params)?
            }
            lsp::notification::Exit::METHOD => self.exit(&notification.params)?,
            // Extensions.
            NOTIFICATION__HandleBufReadPost => {
                self.languageClient_handleBufReadPost(&notification.params)?
            }
            NOTIFICATION__HandleTextChanged => {
                self.languageClient_handleTextChanged(&notification.params)?
            }
            NOTIFICATION__HandleBufWritePost => {
                self.languageClient_handleBufWritePost(&notification.params)?
            }
            NOTIFICATION__HandleBufDelete => {
                self.languageClient_handleBufDelete(&notification.params)?
            }
            NOTIFICATION__HandleCursorMoved => {
                self.languageClient_handleCursorMoved(&notification.params)?
            }
            NOTIFICATION__HandleCompleteDone => {
                self.languageClient_handleCompleteDone(&notification.params)?
            }
            NOTIFICATION__FZFSinkLocation => {
                self.languageClient_FZFSinkLocation(&notification.params)?
            }
            NOTIFICATION__FZFSinkCommand => {
                self.languageClient_FZFSinkCommand(&notification.params)?
            }
            // Extensions by language servers.
            NOTIFICATION__LanguageStatus => self.language_status(&notification.params)?,
            NOTIFICATION__RustBeginBuild => self.rust_handleBeginBuild(&notification.params)?,
            NOTIFICATION__RustDiagnosticsBegin => {
                self.rust_handleDiagnosticsBegin(&notification.params)?
            }
            NOTIFICATION__RustDiagnosticsEnd => {
                self.rust_handleDiagnosticsEnd(&notification.params)?
            }
            NOTIFICATION__WindowProgress => self.window_progress(&notification.params)?,
            NOTIFICATION__CqueryProgress => self.cquery_handleProgress(&notification.params)?,
            NOTIFICATION__ServerExited => self.languageClient_serverExited(&notification.params)?,

            _ => {
                let languageId_target = if languageId.is_some() {
                    // Message from language server. No handler found.
                    let msg = format!("Message not handled: {:?}", notification);
                    if notification.method.starts_with('$') {
                        warn!("{}", msg);
                        return Ok(());
                    } else {
                        return Err(err_msg(msg));
                    }
                } else {
                    // Message from vim. Proxy to language server.
                    let (languageId_target,): (String,) =
                        self.gather_args(&[VimVar::LanguageId], &notification.params)?;
                    Some(languageId_target)
                };

                self.notify(
                    languageId_target.as_deref(),
                    &notification.method,
                    &notification.params,
                )?;
            }
        };

        Ok(())
    }
}
