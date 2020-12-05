use crate::extensions::clangd;
use crate::{language_client::LanguageClient, language_server_protocol::Direction, types::*};
use anyhow::{anyhow, Result};
use log::*;
use lsp_types::notification::{self, Notification};
use lsp_types::request::{self, Request};
use serde_json::Value;

fn is_content_modified_error(err: &anyhow::Error) -> bool {
    match err.downcast_ref::<LSError>() {
        Some(err) if err == &LSError::ContentModified => true,
        _ => false,
    }
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

                    if err.downcast_ref::<LCError>().is_none() {
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
            request::RegisterCapability::METHOD => {
                self.client_register_capability(language_id.unwrap_or_default(), &params)
            }
            request::UnregisterCapability::METHOD => {
                self.client_unregister_capability(language_id.unwrap_or_default(), &params)
            }
            request::HoverRequest::METHOD => self.text_document_hover(&params),
            request::Rename::METHOD => self.text_document_rename(&params),
            request::DocumentSymbolRequest::METHOD => self.text_document_document_symbol(&params),
            request::ShowMessageRequest::METHOD => self.window_show_message_request(&params),
            request::WorkspaceSymbol::METHOD => self.workspace_symbol(&params),
            request::CodeActionRequest::METHOD => self.text_document_code_action(&params),
            request::Completion::METHOD => self.text_document_completion(&params),
            request::SignatureHelpRequest::METHOD => self.text_document_signature_help(&params),
            request::GotoDefinition::METHOD => self.text_document_definition(&params),
            request::References::METHOD => self.text_document_references(&params),
            request::Formatting::METHOD => self.text_document_formatting(&params),
            request::RangeFormatting::METHOD => self.text_document_range_formatting(&params),
            request::CodeLensRequest::METHOD => self.text_document_code_lens(&params),
            request::ResolveCompletionItem::METHOD => self.completion_item_resolve(&params),
            request::ExecuteCommand::METHOD => self.workspace_execute_command(&params),
            request::ApplyWorkspaceEdit::METHOD => self.workspace_apply_edit(&params),
            request::Shutdown::METHOD => self.shutdown(&params),
            request::DocumentHighlightRequest::METHOD => {
                self.text_document_document_highlight(&params)
            }
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
            REQUEST_SEMANTIC_SCOPES => self.semantic_scopes(&params),
            REQUEST_SHOW_SEMANTIC_HL_SYMBOLS => self.semantic_highlight_symbols(&params),
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

        let user_handler =
            self.get_state(|state| state.user_handlers.get(&notification.method).cloned())?;
        if let Some(user_handler) = user_handler {
            return self.vim()?.rpcclient.notify(&user_handler, params);
        }

        match notification.method.as_str() {
            notification::DidChangeConfiguration::METHOD => {
                self.workspace_did_change_configuration(&params)?
            }
            notification::DidOpenTextDocument::METHOD => self.text_document_did_open(&params)?,
            notification::DidChangeTextDocument::METHOD => {
                self.text_document_did_change(&params)?
            }
            notification::DidSaveTextDocument::METHOD => self.text_document_did_save(&params)?,
            notification::DidCloseTextDocument::METHOD => self.text_document_did_close(&params)?,
            notification::PublishDiagnostics::METHOD => {
                self.text_document_publish_diagnostics(&params)?
            }
            notification::SemanticHighlighting::METHOD => {
                self.text_document_semantic_highlight(&params)?
            }
            notification::Progress::METHOD => self.progress(&params)?,
            notification::LogMessage::METHOD => self.window_log_message(&params)?,
            notification::ShowMessage::METHOD => self.window_show_message(&params)?,
            notification::Exit::METHOD => self.exit(&params)?,
            // Extensions.
            NOTIFICATION_HANDLE_FILE_TYPE => self.handle_file_type(&params)?,
            NOTIFICATION_HANDLE_BUF_NEW_FILE => self.handle_buf_new_file(&params)?,
            NOTIFICATION_HANDLE_BUF_ENTER => self.handle_buf_enter(&params)?,
            NOTIFICATION_HANDLE_TEXT_CHANGED => self.handle_text_changed(&params)?,
            NOTIFICATION_HANDLE_BUF_WRITE_POST => self.handle_buf_write_post(&params)?,
            NOTIFICATION_HANDLE_BUF_DELETE => self.handle_buf_delete(&params)?,
            NOTIFICATION_HANDLE_CURSOR_MOVED => self.handle_cursor_moved(&params, false)?,
            NOTIFICATION_HANDLE_COMPLETE_DONE => self.handle_complete_done(&params)?,
            NOTIFICATION_FZF_SINK_LOCATION => self.fzf_sink_location(&params)?,
            NOTIFICATION_FZF_SINK_COMMAND => self.fzf_sink_command(&params)?,
            NOTIFICATION_CLEAR_DOCUMENT_HL => self.clear_document_highlight(&params)?,
            NOTIFICATION_LANGUAGE_STATUS => self.language_status(&params)?,
            NOTIFICATION_WINDOW_PROGRESS => self.window_progress(&params)?,
            NOTIFICATION_SERVER_EXITED => self.handle_server_exited(&params)?,
            NOTIFICATION_RUST_BEGIN_BUILD => self.rust_handle_begin_build(&params)?,
            NOTIFICATION_RUST_DIAGNOSTICS_BEGIN => self.rust_handle_diagnostics_begin(&params)?,
            NOTIFICATION_RUST_DIAGNOSTICS_END => self.rust_handle_diagnostics_end(&params)?,
            NOTIFICATION_DIAGNOSTICS_NEXT => self.cycle_diagnostics(&params, Direction::Next)?,
            NOTIFICATION_DIAGNOSTICS_PREVIOUS => {
                self.cycle_diagnostics(&params, Direction::Previous)?
            }

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
