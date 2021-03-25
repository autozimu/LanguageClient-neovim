use crate::{
    extensions::{clangd, rls},
    lsp::{self, client, completion_item, dollar, text_document, window, workspace},
};
use crate::{language_client::LanguageClient, types::*};
use anyhow::{anyhow, Result};
use log::*;
use lsp_types::notification::{self, Notification};
use lsp_types::request::{self, Request};
use serde_json::Value;

fn is_content_modified_error(err: &anyhow::Error) -> bool {
    matches!(err.downcast_ref::<LanguageServerError>(), Some(err) if err == &LanguageServerError::ContentModified)
}

impl LanguageClient {
    pub fn handle_call(&self, msg: Call) -> Result<()> {
        match msg {
            Call::MethodCall(lang_id, method_call) => {
                let result = self.handle_method_call(lang_id.as_deref(), &method_call);
                if let Err(ref err) = result {
                    if is_content_modified_error(err) {
                        return Ok(());
                    }

                    if err.downcast_ref::<LanguageClientError>().is_none() {
                        error!(
                            "Error handling message: {}\n\nMessage: {}\n\nError: {:?}",
                            err,
                            serde_json::to_string(&method_call).unwrap_or_default(),
                            err
                        );
                    }
                }
                self.get_client(&lang_id)?
                    .output(method_call.id.to_int()?, result)?;
            }
            Call::Notification(lang_id, notification) => {
                let result = self.handle_notification(lang_id.as_deref(), &notification);
                if let Err(ref err) = result {
                    if is_content_modified_error(err) {
                        return Ok(());
                    }

                    if err.downcast_ref::<LanguageClientError>().is_none() {
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

        // FIXME
        if let Err(err) = self.handle_fs_events() {
            warn!("{:?}", err);
        }

        Ok(())
    }

    pub fn handle_method_call(
        &self,
        language_id: Option<&str>,
        method_call: &jsonrpc_core::MethodCall,
    ) -> Result<Value> {
        let params = serde_json::to_value(method_call.params.clone())?;

        let user_handler =
            self.get_state(|state| state.user_handlers.get(&method_call.method).cloned())?;
        if let Some(user_handler) = user_handler {
            return self.vim()?.rpcclient.call(&user_handler, params);
        }

        match method_call.method.as_str() {
            request::HoverRequest::METHOD => text_document::hover(self, &params),
            request::Rename::METHOD => text_document::rename(self, &params),
            request::DocumentSymbolRequest::METHOD => text_document::document_symbol(self, &params),
            request::CodeActionRequest::METHOD => text_document::code_action(self, &params),
            request::Completion::METHOD => text_document::completion(self, &params),
            request::SignatureHelpRequest::METHOD => text_document::signature_help(self, &params),
            request::GotoDefinition::METHOD => text_document::definition(self, &params),
            request::References::METHOD => text_document::references(self, &params),
            request::Formatting::METHOD => text_document::formatting(self, &params),
            request::RangeFormatting::METHOD => text_document::range_formatting(self, &params),
            request::CodeLensRequest::METHOD => text_document::code_lens(self, &params),
            request::SemanticTokensFullRequest::METHOD => {
                text_document::semantic_tokens_full(self, &params)
            }
            request::DocumentHighlightRequest::METHOD => {
                text_document::document_highlight(self, &params)
            }

            request::WorkspaceConfiguration::METHOD => workspace::configuration(self, &params),
            request::WorkspaceSymbol::METHOD => workspace::symbol(self, &params),
            request::ExecuteCommand::METHOD => workspace::execute_command(self, &params),
            request::ApplyWorkspaceEdit::METHOD => workspace::apply_edit(self, &params),

            request::RegisterCapability::METHOD => {
                client::register_capability(self, language_id.unwrap_or_default(), &params)
            }
            request::UnregisterCapability::METHOD => {
                client::unregister_capability(self, language_id.unwrap_or_default(), &params)
            }
            request::ShowMessageRequest::METHOD => window::show_message_request(self, &params),
            request::ResolveCompletionItem::METHOD => completion_item::resolve(self, &params),
            request::Shutdown::METHOD => lsp::shutdown(self, &params),
            // Extensions.
            REQUEST_FIND_LOCATIONS => self.find_locations(&params),
            REQUEST_GET_STATE => self.get_client_state(&params),
            REQUEST_IS_ALIVE => self.is_alive(&params),
            REQUEST_START_SERVER => self.start_server(&params),
            REQUEST_REGISTER_SERVER_COMMANDS => self.register_server_commands(&params),
            REQUEST_SET_LOGGING_LEVEL => self.set_logging_level(&params),
            REQUEST_SET_DIAGNOSTICS_LIST => self.set_diagnostics_list(&params),
            REQUEST_REGISTER_HANDLERS => self.register_handlers(&params),
            REQUEST_NCM_REFRESH => self.ncm_refresh(&params),
            REQUEST_NCM2_ON_COMPLETE => self.ncm2_on_complete(&params),
            REQUEST_EXPLAIN_ERROR_AT_POINT => self.explain_error_at_point(&params),
            REQUEST_OMNI_COMPLETE => self.omnicomplete(&params),
            REQUEST_CLASS_FILE_CONTENTS => self.java_class_file_contents(&params),
            REQUEST_DEBUG_INFO => self.debug_info(&params),
            REQUEST_CODE_LENS_ACTION => self.handle_code_lens_action(&params),
            REQUEST_EXECUTE_CODE_ACTION => self.execute_code_action(&params),

            clangd::request::SwitchSourceHeader::METHOD => {
                self.text_document_switch_source_header(&params)
            }

            _ => {
                let language_id_target = if language_id.is_some() {
                    // Message from language server. No handler found.
                    let msg = format!("Message not handled: {:?}", method_call);
                    if method_call.method.starts_with('$') {
                        warn!("{}", msg);
                        return Ok(Value::default());
                    } else {
                        return Err(anyhow!(msg));
                    }
                } else {
                    // Message from vim. Proxy to language server.
                    let filename = self.vim()?.get_filename(&params)?;
                    let language_id_target = self.vim()?.get_language_id(&filename, &params)?;
                    info!(
                        "Proxy message directly to language server: {:?}",
                        method_call
                    );
                    Some(language_id_target)
                };

                self.get_client(&language_id_target)?
                    .call(&method_call.method, &params)
            }
        }
    }

    pub fn handle_notification(
        &self,
        language_id: Option<&str>,
        notification: &jsonrpc_core::Notification,
    ) -> Result<()> {
        let params = serde_json::to_value(notification.params.clone())?;

        // handle custom, server specific handlers
        let custom_handler = self.get_state(|state| {
            state
                .custom_handlers
                .get(language_id.unwrap_or_default())
                .map(|h| h.get(&notification.method).cloned())
                .unwrap_or_default()
        })?;
        if let Some(handler) = custom_handler {
            return self.vim()?.rpcclient.notify(&handler, params);
        }

        // handle custom, server agnostic handlers
        let user_handler =
            self.get_state(|state| state.user_handlers.get(&notification.method).cloned())?;
        if let Some(user_handler) = user_handler {
            return self.vim()?.rpcclient.notify(&user_handler, params);
        }

        match notification.method.as_str() {
            notification::DidChangeConfiguration::METHOD => {
                workspace::did_change_configuration(self, &params)?
            }
            notification::DidOpenTextDocument::METHOD => {
                text_document::did_open(self, &params)?;
            }
            notification::DidChangeTextDocument::METHOD => {
                text_document::did_change(self, &params)?;
            }
            notification::DidSaveTextDocument::METHOD => {
                text_document::did_save(self, &params)?;
            }
            notification::DidCloseTextDocument::METHOD => text_document::did_close(self, &params)?,
            notification::PublishDiagnostics::METHOD => {
                text_document::publish_diagnostics(self, &params)?
            }
            notification::Progress::METHOD => dollar::progress(self, &params)?,
            notification::LogMessage::METHOD => window::log_message(self, &params)?,
            notification::ShowMessage::METHOD => window::show_message(self, &params)?,
            notification::Exit::METHOD => lsp::exit(self, &params)?,
            // Extensions.
            NOTIFICATION_HANDLE_FILE_TYPE => {
                self.handle_file_type(&params)?;
                text_document::semantic_tokens_full(self, &params)?;
            }
            NOTIFICATION_HANDLE_BUF_NEW_FILE => self.handle_buf_new_file(&params)?,
            NOTIFICATION_HANDLE_BUF_ENTER => {
                self.handle_buf_enter(&params)?;
                text_document::semantic_tokens_full(self, &params)?;
            }
            NOTIFICATION_HANDLE_TEXT_CHANGED => self.handle_text_changed(&params)?,
            NOTIFICATION_HANDLE_BUF_WRITE_POST => {
                self.handle_buf_write_post(&params)?;
                text_document::semantic_tokens_full(self, &params)?;
            }
            NOTIFICATION_HANDLE_BUF_DELETE => self.handle_buf_delete(&params)?,
            NOTIFICATION_HANDLE_CURSOR_MOVED => self.handle_cursor_moved(&params, false)?,
            NOTIFICATION_HANDLE_COMPLETE_DONE => self.handle_complete_done(&params)?,
            NOTIFICATION_FZF_SINK_LOCATION => self.fzf_sink_location(&params)?,
            NOTIFICATION_FZF_SINK_COMMAND => self.fzf_sink_command(&params)?,
            NOTIFICATION_CLEAR_DOCUMENT_HL => self.clear_document_highlight(&params)?,
            NOTIFICATION_LANGUAGE_STATUS => self.language_status(&params)?,
            NOTIFICATION_SERVER_EXITED => self.handle_server_exited(&params)?,
            NOTIFICATION_DIAGNOSTICS_NEXT => self.cycle_diagnostics(&params, Direction::Next)?,
            NOTIFICATION_DIAGNOSTICS_PREVIOUS => {
                self.cycle_diagnostics(&params, Direction::Previous)?
            }

            rls::notification::RUST_DOCUMENT_DIAGNOSTICS_BEGIN => {
                self.rls_handle_diagnostics_begin(&params)?
            }
            rls::notification::RUST_DOCUMENT_DIAGNOSTICS_END => {
                self.rls_handle_diagnostics_end(&params)?
            }
            rls::notification::RUST_DOCUMENT_BEGIN_BUILD => self.rls_handle_begin_build(&params)?,
            rls::notification::WINDOW_PROGRESS => self.rls_window_progress(&params)?,

            _ => {
                let language_id_target = if language_id.is_some() {
                    // Message from language server. No handler found.
                    let msg = format!("Message not handled: {:?}", notification);
                    if notification.method.starts_with('$') {
                        warn!("{}", msg);
                        return Ok(());
                    } else {
                        return Err(anyhow!(msg));
                    }
                } else {
                    // Message from vim. Proxy to language server.
                    let filename = self.vim()?.get_filename(&params)?;
                    let language_id_target = self.vim()?.get_language_id(&filename, &params)?;
                    info!(
                        "Proxy message directly to language server: {:?}",
                        notification
                    );
                    Some(language_id_target)
                };

                self.get_client(&language_id_target)?
                    .notify(&notification.method, &params)?;
            }
        };

        Ok(())
    }
}
