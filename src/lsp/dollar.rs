use crate::language_client::LanguageClient;
use anyhow::Result;
use jsonrpc_core::Value;
use lsp_types::{NumberOrString, ProgressParams, ProgressParamsValue, WorkDoneProgress};
use serde::Deserialize;

#[tracing::instrument(level = "info", skip(lc))]
pub fn progress(lc: &LanguageClient, params: &Value) -> Result<()> {
    let params = ProgressParams::deserialize(params)?;
    let message = match params.value {
        ProgressParamsValue::WorkDone(wd) => match wd {
            WorkDoneProgress::Begin(r) => {
                Some(format!("{} {}", r.title, r.message.unwrap_or_default()))
            }
            WorkDoneProgress::Report(r) => r.message,
            // WorkDoneProgress::End has no value, so we return Done, otherwise the previous
            // message would be left in screen and it would appear as if it didn't ever finish.
            WorkDoneProgress::End(_) => Some("Done".into()),
        },
    };

    if message.is_none() {
        return Ok(());
    }

    let token = match params.token {
        // number is a not a particularly useful token to report to the user, so we just use
        // INFO instead.
        NumberOrString::Number(_) => "INFO".to_string(),
        NumberOrString::String(s) => s,
    };

    let message = format!("{}: {}", token, message.unwrap_or_default());
    lc.vim()?.echomsg(&message)?;
    Ok(())
}
