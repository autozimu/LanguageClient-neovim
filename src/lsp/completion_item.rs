use crate::language_client::LanguageClient;
use crate::vim::try_get;
use anyhow::{anyhow, Result};
use jsonrpc_core::Value;
use lsp_types::{request::Request, CompletionItem, Documentation};
use serde::Deserialize;
use serde_json::json;

#[tracing::instrument(level = "info", skip(lc))]
pub fn resolve(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;
    let has_capability = lc.get_state(|state| match state.capabilities.get(&language_id) {
        None => false,
        Some(result) => result
            .capabilities
            .completion_provider
            .as_ref()
            .map(|cp| cp.resolve_provider.unwrap_or_default())
            .unwrap_or_default(),
    })?;
    if !has_capability {
        return Ok(Value::Null);
    }

    let completion_item: CompletionItem = try_get("completionItem", params)?
        .ok_or_else(|| anyhow!("completionItem not found in request!"))?;
    let pumpos: Value =
        try_get("pumpos", params)?.ok_or_else(|| anyhow!("pumpos not found in request!"))?;

    let result = lc.get_client(&Some(language_id))?.call(
        lsp_types::request::ResolveCompletionItem::METHOD,
        completion_item,
    )?;

    if !lc.vim()?.get_handle(params)? {
        return Ok(result);
    }

    let item = CompletionItem::deserialize(result)?;
    match item.documentation {
        None => return Ok(Value::Null),
        Some(Documentation::String(s)) if s.is_empty() => return Ok(Value::Null),
        Some(Documentation::MarkupContent(m)) if m.value.is_empty() => return Ok(Value::Null),
        _ => lc.vim()?.rpcclient.notify(
            "s:ShowCompletionItemDocumentation",
            json!([item.documentation, pumpos]),
        )?,
    }

    Ok(Value::Null)
}
