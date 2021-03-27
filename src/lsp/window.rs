use crate::language_client::LanguageClient;
use crate::types::ToInt;
use anyhow::Result;
use jsonrpc_core::Value;
use lsp_types::{LogMessageParams, MessageType, ShowMessageRequestParams};
use serde::Deserialize;

// logs a message to with the specified level to the log file if the threshold is below the
// message's level.
#[tracing::instrument(level = "info", skip(lc))]
pub fn log_message(lc: &LanguageClient, params: &Value) -> Result<()> {
    let params = LogMessageParams::deserialize(params)?;
    let threshold = lc.get_config(|c| c.window_log_message_level)?;
    if params.typ.to_int()? > threshold.to_int()? {
        return Ok(());
    }

    match params.typ {
        MessageType::Error => log::error!("{}", params.message),
        MessageType::Warning => log::warn!("{}", params.message),
        MessageType::Info => log::info!("{}", params.message),
        MessageType::Log => log::debug!("{}", params.message),
    };

    Ok(())
}

// shows the given message in vim.
#[tracing::instrument(level = "info", skip(lc))]
pub fn show_message(lc: &LanguageClient, params: &Value) -> Result<()> {
    let params = LogMessageParams::deserialize(params)?;
    let msg = format!("[{:?}] {}", params.typ, params.message);

    match params.typ {
        MessageType::Error => lc.vim()?.echoerr(msg)?,
        MessageType::Warning => lc.vim()?.echowarn(msg)?,
        MessageType::Info => lc.vim()?.echomsg(msg)?,
        MessageType::Log => lc.vim()?.echomsg(msg)?,
    };

    Ok(())
}

// TODO: change this to use the show_acions method
#[tracing::instrument(level = "info", skip(lc))]
pub fn show_message_request(lc: &LanguageClient, params: &Value) -> Result<Value> {
    let mut v = Value::Null;
    let msg_params = ShowMessageRequestParams::deserialize(params)?;
    let msg = format!("[{:?}] {}", msg_params.typ, msg_params.message);
    let msg_actions = msg_params.actions.unwrap_or_default();
    if msg_actions.is_empty() {
        lc.vim()?.echomsg(&msg)?;
    } else {
        let mut options = Vec::with_capacity(msg_actions.len() + 1);
        options.push(msg);
        options.extend(
            msg_actions
                .iter()
                .enumerate()
                .map(|(i, item)| format!("{}) {}", i + 1, item.title)),
        );

        let index: Option<usize> = lc.vim()?.rpcclient.call("s:inputlist", options)?;
        if let Some(index) = index {
            v = serde_json::to_value(msg_actions.get(index - 1))?;
        }
    }

    Ok(v)
}
