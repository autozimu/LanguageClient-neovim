use super::*;
use lsp::request::Request;
use lsp::notification::Notification;

pub trait IRpcHandler {
    fn handle_request(&self, method_call: &rpc::MethodCall) -> Result<Value>;
    fn handle_notification(&self, notification: &rpc::Notification) -> Result<()>;
}

impl IRpcHandler for Arc<Mutex<State>> {
    fn handle_request(&self, method_call: &rpc::MethodCall) -> Result<Value> {
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
            lsp::request::HoverRequest::METHOD => self.textDocument_hover(&method_call.params),
            lsp::request::GotoDefinition::METHOD => {
                self.textDocument_definition(&method_call.params)
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
            REQUEST__OmniComplete => self.languageClient_omniComplete(&method_call.params),
            REQUEST__CqueryBase => self.cquery_base(&method_call.params),
            REQUEST__CqueryCallers => self.cquery_callers(&method_call.params),
            REQUEST__CqueryDerived => self.cquery_derived(&method_call.params),
            REQUEST__CqueryVars => self.cquery_vars(&method_call.params),

            _ => {
                let (languageId,): (String,) =
                    self.gather_args(&[VimVar::LanguageId], &method_call.params)?;

                self.call(
                    Some(languageId.as_str()),
                    &method_call.method,
                    &method_call.params,
                )
            }
        }
    }

    fn handle_notification(&self, notification: &rpc::Notification) -> Result<()> {
        let user_handler = self.get(|state| {
            state
                .user_handlers
                .get(&notification.method)
                .cloned()
                .ok_or_else(|| err_msg("No user handler"))
        });
        if let Ok(user_handler) = user_handler {
            return self.notify(None, &user_handler, notification.params.clone());
        }

        match notification.method.as_str() {
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

            _ => {
                let (languageId,): (String,) =
                    self.gather_args(&[VimVar::LanguageId], &notification.params)?;

                self.notify(
                    Some(languageId.as_str()),
                    &notification.method,
                    &notification.params,
                )?;
            }
        };

        Ok(())
    }
}
