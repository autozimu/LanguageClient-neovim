use super::text_document;
use crate::{language_client::LanguageClient, vim::try_get};
use anyhow::{anyhow, Result};
use jsonrpc_core::Value;
use lsp_types::{
    notification::Notification, request::Request, ApplyWorkspaceEditParams,
    ApplyWorkspaceEditResponse, ConfigurationParams, DidChangeConfigurationParams,
    ExecuteCommandParams, PartialResultParams, SymbolInformation, WorkDoneProgressParams,
    WorkspaceSymbolParams,
};
use serde::Deserialize;

#[tracing::instrument(level = "info", skip(lc))]
pub fn symbol(lc: &LanguageClient, params: &Value) -> Result<Value> {
    text_document::did_change(lc, params)?;
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;

    let query = try_get("query", params)?.unwrap_or_default();
    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::WorkspaceSymbol::METHOD,
        WorkspaceSymbolParams {
            query,
            partial_result_params: PartialResultParams::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }

    let symbols = <Vec<SymbolInformation>>::deserialize(&result)?;
    let title = "[LC]: workspace symbols";
    lc.present_list(title, &symbols)?;
    Ok(result)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn execute_command(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let command: String =
        try_get("command", params)?.ok_or_else(|| anyhow!("command not found in request!"))?;
    let arguments: Vec<Value> = try_get("arguments", params)?.unwrap_or_default();

    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::ExecuteCommand::METHOD,
        ExecuteCommandParams {
            command,
            arguments,
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )?;
    Ok(result)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn apply_edit(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let params = ApplyWorkspaceEditParams::deserialize(params)?;
    lc.apply_workspace_edit(&params.edit)?;
    Ok(serde_json::to_value(ApplyWorkspaceEditResponse {
        applied: true,
        failure_reason: None,
        failed_change: None,
    })?)
}

pub fn did_change_configuration(lc: &LanguageClient, params: &Value) -> Result<()> {
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let settings: Value = try_get("settings", params)?.unwrap_or_default();

    lc.get_client(&Some(language_id))?.notify(
        lsp_types::notification::DidChangeConfiguration::METHOD,
        DidChangeConfigurationParams { settings },
    )?;
    Ok(())
}

pub fn configuration(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let config_params = ConfigurationParams::deserialize(params)?;
    let settings = lc.get_state(|state| state.initialization_options.clone())?;
    let configuration_items = config_params
        .items
        .into_iter()
        .filter_map(|item| {
            let section = format!("/{}", item.section?.replace(".", "/"));
            settings.pointer(&section).cloned()
        })
        .collect::<Value>();

    Ok(configuration_items)
}
