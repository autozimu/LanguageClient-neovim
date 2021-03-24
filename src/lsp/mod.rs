pub mod client;
pub mod completion_item;
pub mod dollar;
pub mod text_document;
pub mod window;
pub mod workspace;

use crate::language_client::LanguageClient;
use crate::types::VIM_IS_SERVER_RUNNING;
use anyhow::Result;
use jsonrpc_core::Value;
use lsp_types::{notification::Notification, request::Request};
use serde_json::json;

#[tracing::instrument(level = "info", skip(lc))]
pub fn shutdown(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;

    let _: () = lc
        .get_client(&Some(language_id))?
        .call(lsp_types::request::Shutdown::METHOD, Value::Null)?;

    lc.vim()?
        .rpcclient
        .notify("setbufvar", json!([filename, VIM_IS_SERVER_RUNNING, 0]))?;

    Ok(Value::Null)
}

#[tracing::instrument(level = "info", skip(lc))]
pub fn exit(lc: &LanguageClient, params: &Value) -> Result<()> {
    let filename = lc.vim()?.get_filename(params)?;
    let language_id = lc.vim()?.get_language_id(&filename, params)?;

    let result = lc
        .get_client(&Some(language_id.clone()))?
        .notify(lsp_types::notification::Exit::METHOD, Value::Null);
    if let Err(err) = result {
        log::error!("Error: {:?}", err);
    }

    if let Err(err) = lc.cleanup(&language_id) {
        log::error!("Error: {:?}", err);
    }

    Ok(())
}
