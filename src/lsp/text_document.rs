use crate::{
    config::SemanticTokenMapping,
    language_client::LanguageClient,
    types::{
        Bufnr, ClearNamespace, DiagnosticSeverityExt, Filepath, HoverPreviewOption, LCNamespace,
        LinesLen, TextDocumentItemMetadata, ToDisplay, ToInt, ToString, UseVirtualText,
        VIM_STATUS_LINE_DIAGNOSTICS_COUNTS,
    },
    utils::{decode_parameter_label, Canonicalize, Combine, ToUrl},
    vim::{try_get, Highlight},
};
use anyhow::{anyhow, Result};
use jsonrpc_core::Value;
use lsp_types::{
    notification::Notification, request::Request, CodeAction, CodeActionOrCommand,
    CodeActionResponse, CodeLens, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentFormattingParams, DocumentHighlight, DocumentHighlightKind,
    DocumentRangeFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, FormattingOptions,
    Hover, ParameterInformation, PartialResultParams, Position, PublishDiagnosticsParams, Range,
    ReferenceContext, RenameParams, SemanticToken, SemanticTokenModifier, SemanticTokensLegend,
    SemanticTokensParams, SemanticTokensResult, SignatureHelp, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, VersionedTextDocumentIdentifier,
    WorkDoneProgressParams, WorkspaceEdit,
};
use maplit::hashmap;
use serde::Deserialize;
use serde_json::json;
use std::{collections::HashMap, time::Instant};

#[tracing::instrument(level = "info", skip(lc))]
pub fn did_change(lc: &LanguageClient, params: &Value) -> Result<()> {
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    if !lc.get_state(|state| state.text_documents.contains_key(&filename))? {
        log::info!("Not opened yet. Switching to didOpen.");
        return did_open(lc, params);
    }

    let text = lc.vim()?.get_text(&filename)?.join("\n");
    let text_state = lc.get_state(|state| {
        state
            .text_documents
            .get(&filename)
            .map(|d| d.text.clone())
            .unwrap_or_default()
    })?;
    if text == text_state {
        return Ok(());
    }

    let change_throttle = lc.get_config(|c| c.change_throttle.is_some())?;
    let version = lc.update_state(|state| {
        let document = state
            .text_documents
            .get_mut(&filename)
            .ok_or_else(|| anyhow!("Failed to get TextDocumentItem! filename: {}", filename))?;

        let version = document.version + 1;
        document.version = version;
        document.text = text.clone();

        if change_throttle {
            let metadata = state
                .text_documents_metadata
                .entry(filename.clone())
                .or_insert_with(TextDocumentItemMetadata::default);
            metadata.last_change = Instant::now();
        }
        Ok(version)
    })?;

    lc.get_client(&Some(language_id.clone()))?.notify(
        lsp_types::notification::DidChangeTextDocument::METHOD,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: filename.to_url()?,
                version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text,
            }],
        },
    )?;

    code_lens(lc, params)?;
    lc.text_document_inlay_hints(&language_id, &filename)?;

    Ok(())
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn did_save(lc: &LanguageClient, params: &Value) -> Result<()> {
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let has_capability = lc.get_state(|s| {
        s.capabilities
            .get(&language_id)
            .as_ref()
            .map(|i| match i.capabilities.text_document_sync.as_ref() {
                Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::None)) => false,
                Some(TextDocumentSyncCapability::Kind(_)) => true,
                Some(TextDocumentSyncCapability::Options(o)) => match o.save {
                    Some(lsp_types::TextDocumentSyncSaveOptions::Supported(b)) => b,
                    Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(_)) => true,
                    None => false,
                },
                None => false,
            })
            .unwrap_or_default()
    })?;
    if !has_capability {
        return Ok(());
    }

    if !lc.get_config(|c| c.server_commands.contains_key(&language_id))? {
        return Ok(());
    }

    let uri = filename.to_url()?;

    lc.get_client(&Some(language_id))?.notify(
        lsp_types::notification::DidSaveTextDocument::METHOD,
        DidSaveTextDocumentParams {
            text: None,
            text_document: TextDocumentIdentifier { uri },
        },
    )?;

    lc.draw_virtual_texts(params)?;

    Ok(())
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn did_close(lc: &LanguageClient, params: &Value) -> Result<()> {
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;

    lc.get_client(&Some(language_id))?.notify(
        lsp_types::notification::DidCloseTextDocument::METHOD,
        DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
        },
    )?;
    Ok(())
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn publish_diagnostics(lc: &LanguageClient, params: &Value) -> Result<()> {
    let params = PublishDiagnosticsParams::deserialize(params)?;
    if !lc.get_config(|c| c.diagnostics_enable)? {
        return Ok(());
    }

    let mut filename = params.uri.filepath()?.to_string_lossy().into_owned();
    // Workaround bug: remove first '/' in case of '/C:/blabla'.
    if filename.starts_with('/') && filename.chars().nth(2) == Some(':') {
        filename.remove(0);
    }
    // Unify name to avoid mismatch due to case insensitivity.
    let filename = filename.canonicalize();

    let diagnostics_max_severity = lc.get_config(|c| c.diagnostics_max_severity)?;
    let ignore_sources = lc.get_config(|c| c.diagnostics_ignore_sources.clone())?;
    let mut diagnostics = params
        .diagnostics
        .iter()
        .filter(|&diagnostic| {
            if let Some(source) = &diagnostic.source {
                if ignore_sources.contains(source) {
                    return false;
                }
            }
            diagnostic.severity.unwrap_or(DiagnosticSeverity::Hint) <= diagnostics_max_severity
        })
        .map(Clone::clone)
        .collect::<Vec<_>>();

    lc.update_state(|state| {
        state
            .diagnostics
            .insert(filename.clone(), diagnostics.clone());
        Ok(())
    })?;
    lc.update_quickfixlist()?;

    let mut severity_count: HashMap<String, u64> = [
        (
            DiagnosticSeverity::Error
                .to_quickfix_entry_type()
                .to_string(),
            0,
        ),
        (
            DiagnosticSeverity::Warning
                .to_quickfix_entry_type()
                .to_string(),
            0,
        ),
        (
            DiagnosticSeverity::Information
                .to_quickfix_entry_type()
                .to_string(),
            0,
        ),
        (
            DiagnosticSeverity::Hint
                .to_quickfix_entry_type()
                .to_string(),
            0,
        ),
    ]
    .iter()
    .cloned()
    .collect();

    for diagnostic in diagnostics.iter() {
        let severity = diagnostic
            .severity
            .unwrap_or(DiagnosticSeverity::Hint)
            .to_quickfix_entry_type()
            .to_string();
        let count = severity_count.entry(severity).or_insert(0);
        *count += 1;
    }

    if let Ok(bufnr) = lc.vim()?.eval::<_, Bufnr>(format!("bufnr('{}')", filename)) {
        // Some Language Server diagnoses non-opened buffer, so we must check if buffer exists.
        if bufnr > 0 {
            lc.vim()?.rpcclient.notify(
                "setbufvar",
                json!([filename, VIM_STATUS_LINE_DIAGNOSTICS_COUNTS, severity_count]),
            )?;
        }
    }

    let current_filename: String = lc.vim()?.get_filename(&Value::Null)?;
    if filename != current_filename.canonicalize() {
        return Ok(());
    }

    // Sort diagnostics as pre-process for display.
    // First sort by line.
    // Then severity descending. Error should come last since when processing item comes
    // later will override its precedence.
    // Then by character descending.
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.range.start.line,
            -(diagnostic.severity.unwrap_or(DiagnosticSeverity::Hint) as i8),
            -(diagnostic.range.start.line as i64),
        )
    });

    lc.process_diagnostics(&current_filename, &diagnostics)?;
    lc.handle_cursor_moved(&Value::Null, true)?;
    lc.vim()?
        .rpcclient
        .notify("s:ExecuteAutocmd", "LanguageClientDiagnosticsChanged")?;

    Ok(())
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn document_highlight(lc: &LanguageClient, params: &Value) -> Result<Value> {
    did_change(lc, params)?;
    let filename = lc.vim()?.get_filename(&Value::Null)?;
    let language_id = lc.vim()?.get_language_id(&filename, &Value::Null)?;
    let position = lc.vim()?.get_position(&Value::Null)?;

    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::DocumentHighlightRequest::METHOD,
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            position,
        },
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }

    let document_highlight = <Option<Vec<DocumentHighlight>>>::deserialize(&result)?;
    if let Some(document_highlight) = document_highlight {
        let document_highlight_display = lc.get_config(|c| c.document_highlight_display.clone())?;
        let highlights = document_highlight
            .into_iter()
            .map(|DocumentHighlight { range, kind }| {
                Ok(Highlight {
                    line: range.start.line,
                    character_start: range.start.character,
                    character_end: range.end.character,
                    group: document_highlight_display
                        .get(
                            &kind
                                .unwrap_or(DocumentHighlightKind::Text)
                                .to_int()
                                .unwrap(),
                        )
                        .ok_or_else(|| anyhow!("Failed to get display"))?
                        .texthl
                        .clone(),
                    text: String::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        lc.vim()?
            .set_highlights(&highlights, "__LCN_DOCUMENT_HIGHLIGHT__")?;
    }

    Ok(result)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn hover(lc: &LanguageClient, params: &Value) -> Result<Value> {
    did_change(lc, params)?;
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let position = lc.vim()?.get_position(params)?;

    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::HoverRequest::METHOD,
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            position,
        },
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }

    let hover = Option::<Hover>::deserialize(&result)?;
    if let Some(hover) = hover {
        if hover.to_display().is_empty() {
            lc.vim()?
                .echowarn("No hover information found for symbol")?;
            return Ok(Value::Null);
        }

        let hover_preview = lc.get_config(|c| c.hover_preview)?;
        let use_preview = match hover_preview {
            HoverPreviewOption::Always => true,
            HoverPreviewOption::Never => false,
            HoverPreviewOption::Auto => hover.lines_len() > 1,
        };
        if use_preview {
            lc.preview(&hover, "__LCNHover__")?
        } else {
            lc.vim()?.echo_ellipsis(hover.to_string())?
        }
    }

    Ok(result)
}
#[tracing::instrument(level = "info", skip(lc))]
pub fn rename(lc: &LanguageClient, params: &Value) -> Result<Value> {
    did_change(lc, params)?;
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let position = lc.vim()?.get_position(params)?;
    let current_word = lc.vim()?.get_current_word(params)?;
    let new_name: Option<String> = try_get("newName", params)?;

    let mut new_name = new_name.unwrap_or_default();
    if new_name.is_empty() {
        new_name = lc
            .vim()?
            .rpcclient
            .call("s:getInput", ["Rename to: ", &current_word])?;
    }
    if new_name.is_empty() {
        return Ok(Value::Null);
    }

    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::Rename::METHOD,
        RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                position,
            },
            new_name,
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }
    if result == Value::Null {
        return Ok(result);
    }

    let edit = WorkspaceEdit::deserialize(&result)?;
    lc.apply_workspace_edit(&edit)?;

    Ok(result)
}
#[tracing::instrument(level = "info", skip(lc))]
pub fn document_symbol(lc: &LanguageClient, params: &Value) -> Result<Value> {
    did_change(lc, params)?;
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;

    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::DocumentSymbolRequest::METHOD,
        DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }

    let syms = <Option<DocumentSymbolResponse>>::deserialize(&result)?;
    let title = format!("[LC]: symbols for {}", filename);

    match syms {
        Some(DocumentSymbolResponse::Flat(flat)) => {
            lc.present_list(&title, &flat)?;
        }
        Some(DocumentSymbolResponse::Nested(nested)) => {
            let mut symbols = Vec::new();

            fn walk_document_symbol(
                buffer: &mut Vec<lsp_types::DocumentSymbol>,
                parent: Option<&lsp_types::DocumentSymbol>,
                ds: &lsp_types::DocumentSymbol,
            ) {
                let name = if let Some(parent) = parent {
                    format!("{}::{}", parent.name, ds.name)
                } else {
                    ds.name.clone()
                };

                buffer.push(lsp_types::DocumentSymbol { name, ..ds.clone() });

                if let Some(children) = &ds.children {
                    for child in children {
                        walk_document_symbol(buffer, Some(&ds), child);
                    }
                }
            }

            for ds in &nested {
                walk_document_symbol(&mut symbols, None, ds);
            }

            lc.present_list(&title, &symbols)?;
        }
        _ => (),
    };

    Ok(result)
}
#[tracing::instrument(level = "info", skip(client))]
pub fn code_action(client: &LanguageClient, params: &Value) -> Result<Value> {
    let result = client.get_code_actions(params)?;
    let response = <Option<CodeActionResponse>>::deserialize(&result)?;
    let response = response.unwrap_or_default();

    // Convert any Commands into CodeActions, so that the remainder of the handling can be
    // shared.
    let actions: Vec<_> = response
        .into_iter()
        .map(|action_or_command| match action_or_command {
            CodeActionOrCommand::Command(command) => CodeAction {
                title: command.title.clone(),
                kind: Some(command.command.clone().into()),
                diagnostics: None,
                edit: None,
                command: Some(command),
                ..CodeAction::default()
            },
            CodeActionOrCommand::CodeAction(action) => action,
        })
        .collect();

    client.update_state(|state| {
        state.stashed_code_action_actions = actions.clone();
        Ok(())
    })?;

    if !client.vim()?.get_handle(params)? {
        return Ok(result);
    }

    client.present_actions("Code Actions", &actions, |idx| -> Result<()> {
        client.handle_code_action_selection(&actions, idx)
    })?;

    Ok(result)
}

#[tracing::instrument(level = "info", skip(client))]
pub fn completion(client: &LanguageClient, params: &Value) -> Result<Value> {
    let filename = client.vim()?.get_filename(params)?;
    let language_id = client.vim()?.get_language_id(&filename, params)?;
    let position = client.vim()?.get_position(params)?;

    let result = client.get_client(&Some(language_id))?.call(
        lsp_types::request::Completion::METHOD,
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            position,
        },
    )?;

    if !client.vim()?.get_handle(params)? {
        return Ok(result);
    }

    Ok(result)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn signature_help(lc: &LanguageClient, params: &Value) -> Result<Value> {
    did_change(lc, params)?;
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let position = lc.vim()?.get_position(params)?;

    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::SignatureHelpRequest::METHOD,
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            position,
        },
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }
    if result == Value::Null {
        return Ok(result);
    }

    let help = SignatureHelp::deserialize(result)?;
    if help.signatures.is_empty() {
        return Ok(Value::Null);
    }

    // active_signature may be negative value.
    // So if it is negative value, we convert it into zero.
    let active_signature_index = help.active_signature.unwrap_or(0).max(0) as usize;

    let active_signature = help
        .signatures
        .get(active_signature_index)
        .ok_or_else(|| anyhow!("Failed to get active signature"))?;

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
        decode_parameter_label(&active_parameter.label, &active_signature.label).ok()
    }) {
        let cmd = format!(
            "echo | echon '{}' | echohl WarningMsg | echon '{}' | echohl None | echon '{}'",
            begin, label, end
        );
        lc.vim()?.command(&cmd)?;
    } else {
        lc.vim()?.echo(&active_signature.label)?;
    }

    Ok(Value::Null)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn definition(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let params = json!({
        "method": lsp_types::request::GotoDefinition::METHOD,
    })
    .combine(params);
    let result = lc.find_locations(&params)?;
    Ok(result)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn references(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let include_declaration: bool = try_get("includeDeclaration", params)?.unwrap_or(true);
    let params = json!({
        "method": lsp_types::request::References::METHOD,
        "context": ReferenceContext {
            include_declaration,
        }
    })
    .combine(params);
    let result = lc.find_locations(&params)?;
    Ok(result)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn formatting(lc: &LanguageClient, params: &Value) -> Result<Value> {
    did_change(lc, params)?;
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;

    let tab_size = lc.vim()?.get_tab_size()?;
    let insert_spaces = lc.vim()?.get_insert_spaces(&filename)?;
    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::Formatting::METHOD,
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            options: FormattingOptions {
                tab_size,
                insert_spaces,
                properties: HashMap::new(),
                ..FormattingOptions::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }

    let text_edits = <Option<Vec<TextEdit>>>::deserialize(&result)?;
    let text_edits = text_edits.unwrap_or_default();
    let edit = lsp_types::WorkspaceEdit {
        changes: Some(hashmap! {filename.to_url()? => text_edits}),
        change_annotations: None,
        document_changes: None,
    };
    lc.apply_workspace_edit(&edit)?;
    Ok(result)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn range_formatting(lc: &LanguageClient, params: &Value) -> Result<Value> {
    did_change(lc, params)?;
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let start_line = try_get("range_start_line", params)?
        .map_or_else(|| lc.vim()?.eval("LSP#range_start_line()"), Ok)?;
    let end_line = try_get("range_end_line", params)?
        .map_or_else(|| lc.vim()?.eval("LSP#range_end_line()"), Ok)?;

    let tab_size = lc.vim()?.get_tab_size()?;
    let insert_spaces = lc.vim()?.get_insert_spaces(&filename)?;
    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::RangeFormatting::METHOD,
        DocumentRangeFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            options: FormattingOptions {
                tab_size,
                insert_spaces,
                properties: HashMap::new(),
                ..FormattingOptions::default()
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
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }

    let text_edits = <Option<Vec<TextEdit>>>::deserialize(&result)?;
    let text_edits = text_edits.unwrap_or_default();
    let edit = lsp_types::WorkspaceEdit {
        changes: Some(hashmap! {filename.to_url()? => text_edits}),
        change_annotations: None,
        document_changes: None,
    };
    lc.apply_workspace_edit(&edit)?;
    Ok(result)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn code_lens(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let use_virtual_text = lc.get_config(|c| c.use_virtual_text.clone())?;
    if UseVirtualText::No == use_virtual_text || UseVirtualText::Diagnostics == use_virtual_text {
        return Ok(Value::Null);
    }

    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let capabilities = lc.get_state(|state| state.capabilities.clone())?;
    if let Some(initialize_result) = capabilities.get(&language_id) {
        // XXX: the capabilities state field stores the initialize result, not the capabilities
        // themselves, so we need to deserialize to InitializeResult.
        let capabilities = initialize_result.capabilities.clone();

        if let Some(code_lens_provider) = capabilities.code_lens_provider {
            let client = lc.get_client(&Some(language_id))?;
            let input = lsp_types::CodeLensParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_url()?,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            };

            let results: Value =
                client.call(lsp_types::request::CodeLensRequest::METHOD, &input)?;
            let code_lens = <Option<Vec<CodeLens>>>::deserialize(results)?;
            let mut code_lens: Vec<CodeLens> = code_lens.unwrap_or_default();

            if code_lens_provider.resolve_provider.unwrap_or_default() {
                code_lens = code_lens
                    .into_iter()
                    .map(|cl| {
                        if cl.data.is_none() {
                            return cl;
                        }

                        client
                            .call(lsp_types::request::CodeLensResolve::METHOD, &cl)
                            .unwrap_or(cl)
                    })
                    .collect();
            }

            lc.update_state(|state| {
                state.code_lens.insert(filename.to_owned(), code_lens);
                Ok(Value::Null)
            })?;
        }
    }

    lc.draw_virtual_texts(&params)?;

    Ok(Value::Null)
}

#[tracing::instrument(level = "info", skip(client))]
pub fn did_open(client: &LanguageClient, params: &Value) -> Result<()> {
    let filename = client.vim()?.get_filename(params)?;
    let language_id = client.vim()?.get_language_id(&filename, params)?;
    let text = client.vim()?.get_text(&filename)?;
    let set_omnifunc: bool = client
        .vim()?
        .eval("s:GetVar('LanguageClient_setOmnifunc', v:true)")?;

    let text_document = TextDocumentItem {
        uri: filename.to_url()?,
        language_id: language_id.clone(),
        version: 0,
        text: text.join("\n"),
    };

    client.update_state(|state| {
        Ok(state
            .text_documents
            .insert(filename.clone(), text_document.clone()))
    })?;

    client.get_client(&Some(language_id.clone()))?.notify(
        lsp_types::notification::DidOpenTextDocument::METHOD,
        DidOpenTextDocumentParams { text_document },
    )?;

    if set_omnifunc {
        client
            .vim()?
            .command("setlocal omnifunc=LanguageClient#complete")?;
    }
    let root =
        client.get_state(|state| state.roots.get(&language_id).cloned().unwrap_or_default())?;
    client.vim()?.rpcclient.notify(
        "setbufvar",
        json!([filename, "LanguageClient_projectRoot", root]),
    )?;
    client
        .vim()?
        .rpcclient
        .notify("s:ExecuteAutocmd", "LanguageClientTextDocumentDidOpenPost")?;

    code_lens(client, params)?;
    client.text_document_inlay_hints(&language_id, &filename)?;

    Ok(())
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn semantic_tokens_full(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let is_enabled = lc
        .get_config(|c| c.semantic_highlighting_enabled)
        .unwrap_or_default();
    if !is_enabled {
        return Ok(Value::Null);
    }

    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let client = lc.get_client(&Some(language_id.clone()))?;
    let response: SemanticTokensResult = client.call(
        lsp_types::request::SemanticTokensFullRequest::METHOD,
        SemanticTokensParams {
            text_document: TextDocumentIdentifier {
                uri: filename.to_url()?,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )?;

    let legend = lc.get_state(|s| s.semantic_token_legends.get(&language_id).cloned())?;
    if legend.is_none() {
        return Ok(Value::Null);
    }
    let legend = legend.unwrap();

    let mappings = lc
        .get_config(|c| c.semantic_token_mappings.clone())
        .unwrap_or_default();
    if mappings.is_empty() {
        return Ok(Value::Null);
    }

    let highlights: Vec<Highlight> = match response {
        SemanticTokensResult::Tokens(r) => semantic_tokens_to_highlights(&legend, mappings, r.data),
        SemanticTokensResult::Partial(_) => vec![],
    };

    let ns_id = lc.get_or_create_namespace(&LCNamespace::SemanticHighlight)?;
    let buffer = lc.vim()?.get_bufnr(&filename, &Value::Null)?;
    let clears: Vec<ClearNamespace> = Vec::new();
    lc.vim()?.rpcclient.notify(
        "s:ApplySemanticHighlights",
        json!([buffer, ns_id, clears, highlights]),
    )?;

    Ok(Value::Null)
}

fn semantic_tokens_to_highlights(
    legend: &SemanticTokensLegend,
    mappings: Vec<SemanticTokenMapping>,
    tokens: Vec<SemanticToken>,
) -> Vec<Highlight> {
    tokens
        .iter()
        .enumerate()
        .filter_map(|(idx, d)| {
            let token_modifiers = resolve_token_modifiers(&legend, d.token_modifiers_bitset);
            let mapping = resolve_token_mapping(&legend, &mappings, &token_modifiers, d);
            let line: u32 = tokens.iter().take(idx + 1).map(|i| i.delta_line).sum();
            let mut character_start = d.delta_start;
            // loop backwards over the results for as long as we find that the previous token also
            // has a delta_line of zero.
            if idx > 0 && d.delta_line == 0 {
                let mut inner_idx = idx;
                loop {
                    character_start += tokens[inner_idx - 1].delta_start;
                    if inner_idx - 1 == 0 || tokens[inner_idx - 1].delta_line != 0 {
                        break;
                    }
                    inner_idx -= 1;
                }
            }
            mapping.map(|m| Highlight {
                line,
                character_start,
                character_end: character_start + d.length,
                group: m.highlight_group.clone(),
                text: m.highlight_group,
            })
        })
        .collect()
}

/// LSP uses a bitset to indicate which token modifiers apply for a given token. The actual
/// modifiers are obtained from the legend at the indexes indicated by the bits set to 1 in the
/// bitset.
fn resolve_token_modifiers(
    legend: &SemanticTokensLegend,
    bitset: u32,
) -> Vec<SemanticTokenModifier> {
    format!("{:#b}", bitset)
        .replace("0b", "")
        .chars()
        .into_iter()
        .rev()
        .enumerate()
        .filter_map(|(idx, c)| match c.to_string().parse::<usize>().unwrap() {
            0 => None,
            _ => Some(legend.token_modifiers[idx].clone()),
        })
        .collect()
}

/// LSP uses integers to indicate an encoded semantic token type that corresponds to each token, in
/// order to get the name of the type we must index the token_types field of the legend with the
/// number that represents the encoded token type.
fn resolve_token_mapping(
    legend: &SemanticTokensLegend,
    mappings: &[SemanticTokenMapping],
    token_modifiers: &[SemanticTokenModifier],
    token: &SemanticToken,
) -> Option<SemanticTokenMapping> {
    let token_type = legend.token_types[token.token_type as usize].clone();
    let mappings: Vec<SemanticTokenMapping> = mappings
        .iter()
        .filter(|i| i.name == token_type.as_str())
        .cloned()
        .collect();
    if mappings.is_empty() {
        return None;
    }

    // get either the mapping that matches both type and modifiers
    let modifiers: Vec<&str> = token_modifiers.iter().map(|i| i.as_str()).collect();
    mappings.iter().find(|m| modifiers == m.modifiers).cloned()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::*;
    use lsp_types::*;

    #[test]
    fn it_resolves_the_correct_mapping() {
        let legend = SemanticTokensLegend {
            token_types: vec![SemanticTokenType::CLASS, SemanticTokenType::FUNCTION],
            token_modifiers: vec![],
        };
        let mappings = vec![
            SemanticTokenMapping::new("function", &[], "Function"),
            SemanticTokenMapping::new("comment", &[], "Comment"),
        ];
        let actual = resolve_token_mapping(
            &legend,
            &mappings,
            &[],
            &SemanticToken {
                token_type: 1,
                delta_line: 0,
                delta_start: 0,
                length: 0,
                token_modifiers_bitset: 0,
            },
        );
        let expect = SemanticTokenMapping::new("function", &[], "Function");

        assert!(actual.is_some());
        assert_eq!(expect, actual.unwrap());
    }

    #[test]
    fn it_resolves_to_the_correct_modifiers() {
        let legend = SemanticTokensLegend {
            token_types: vec![],
            token_modifiers: vec![
                SemanticTokenModifier::DEPRECATED,
                SemanticTokenModifier::DECLARATION,
                SemanticTokenModifier::DOCUMENTATION,
                SemanticTokenModifier::ASYNC,
            ],
        };

        // if the token_modifier is 3 then it translates to a 0b0011 bitset, which means we want
        // the modifiers in index 0 and 1 of the legend.
        let actual = resolve_token_modifiers(&legend, 3);
        let expect = vec![
            SemanticTokenModifier::DEPRECATED,
            SemanticTokenModifier::DECLARATION,
        ];
        assert_eq!(expect, actual);

        // if the token_modifier is 2 then it translates to a 0b0010 bitset, which means we want
        // the modifiers in index 1 of the legend.
        let actual = resolve_token_modifiers(&legend, 2);
        let expect = vec![SemanticTokenModifier::DECLARATION];
        assert_eq!(expect, actual);
    }

    #[test]
    fn it_maps_the_example_in_the_spec() {
        // inputs:
        //      { deltaLine: 2, deltaStartChar: 5, length: 3, tokenType: 0, tokenModifiers: 3 },
        //      { deltaLine: 0, deltaStartChar: 5, length: 4, tokenType: 1, tokenModifiers: 0 },
        //      { deltaLine: 3, deltaStartChar: 2, length: 7, tokenType: 2, tokenModifiers: 0 }
        //
        // outputs:
        //      { line: 2, startChar:  5, length: 3, tokenType: 0, tokenModifiers: 3 },
        //      { line: 2, startChar: 10, length: 4, tokenType: 1, tokenModifiers: 0 },
        //      { line: 5, startChar:  2, length: 7, tokenType: 2, tokenModifiers: 0 }
        //
        let tokens = vec![
            SemanticToken {
                delta_line: 2,
                delta_start: 5,
                length: 3,
                token_type: 1,
                token_modifiers_bitset: 0,
            },
            SemanticToken {
                delta_line: 0,
                delta_start: 5,
                length: 4,
                token_type: 1,
                token_modifiers_bitset: 0,
            },
            SemanticToken {
                delta_line: 3,
                delta_start: 2,
                length: 7,
                token_type: 1,
                token_modifiers_bitset: 0,
            },
        ];
        let legend = SemanticTokensLegend {
            token_types: vec![SemanticTokenType::FUNCTION, SemanticTokenType::TYPE],
            token_modifiers: vec![],
        };
        let mappings = vec![
            SemanticTokenMapping::new("function", &[], "Function"),
            SemanticTokenMapping::new("type", &[], "Type"),
        ];
        let actual = semantic_tokens_to_highlights(&legend, mappings, tokens);
        let expect = vec![
            Highlight {
                line: 2,
                character_start: 5,
                character_end: 8,
                group: "Type".into(),
                text: "Type".into(),
            },
            Highlight {
                line: 2,
                character_start: 10,
                character_end: 14,
                group: "Type".into(),
                text: "Type".into(),
            },
            Highlight {
                line: 5,
                character_start: 2,
                character_end: 9,
                group: "Type".into(),
                text: "Type".into(),
            },
        ];
        assert_eq!(expect, actual);
    }
}
