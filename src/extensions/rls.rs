use crate::language_client::LanguageClient;
use crate::types::{VIM_SERVER_STATUS, VIM_SERVER_STATUS_MESSAGE};
use crate::utils::escape_single_quote;
use anyhow::Result;
use jsonrpc_core::Value;
use serde::{Deserialize, Serialize};

pub mod notification {
    pub const WINDOW_PROGRESS: &str = "window/progress";
    pub const RUST_DOCUMENT_BEGIN_BUILD: &str = "rustDocument/beginBuild";
    pub const RUST_DOCUMENT_DIAGNOSTICS_BEGIN: &str = "rustDocument/diagnosticsBegin";
    pub const RUST_DOCUMENT_DIAGNOSTICS_END: &str = "rustDocument/diagnosticsEnd";
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WindowProgressParams {
    pub title: Option<String>,
    pub message: Option<String>,
    pub percentage: Option<f64>,
    pub done: Option<bool>,
}

impl LanguageClient {
    #[tracing::instrument(level = "info", skip(self))]
    pub fn rls_window_progress(&self, params: &Value) -> Result<()> {
        let params = WindowProgressParams::deserialize(params)?;

        let done = params.done.unwrap_or(false);

        let mut buf = "LS: ".to_owned();

        if done {
            buf += "Idle";
        } else {
            // For RLS this can be "Build" or "Diagnostics" or "Indexing".
            buf += params.title.as_ref().map(AsRef::as_ref).unwrap_or("Busy");

            // For RLS this is the crate name, present only if the progress isn't known.
            if let Some(message) = params.message {
                buf += &format!(" ({})", &message);
            }
            // For RLS this is the progress percentage, present only if the it's known.
            if let Some(percentage) = params.percentage {
                buf += &format!(" ({:.1}% done)", percentage);
            }
        }

        self.vim()?.command(vec![
            format!("let {}={}", VIM_SERVER_STATUS, if done { 0 } else { 1 }),
            format!(
                "let {}='{}'",
                VIM_SERVER_STATUS_MESSAGE,
                &escape_single_quote(buf)
            ),
        ])?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn rls_handle_begin_build(&self, _params: &Value) -> Result<()> {
        self.vim()?.command(vec![
            format!("let {}=1", VIM_SERVER_STATUS),
            format!("let {}='Rust: build begin'", VIM_SERVER_STATUS_MESSAGE),
        ])?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn rls_handle_diagnostics_begin(&self, _params: &Value) -> Result<()> {
        self.vim()?.command(vec![
            format!("let {}=1", VIM_SERVER_STATUS),
            format!(
                "let {}='Rust: diagnostics begin'",
                VIM_SERVER_STATUS_MESSAGE
            ),
        ])?;
        Ok(())
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub fn rls_handle_diagnostics_end(&self, _params: &Value) -> Result<()> {
        self.vim()?.command(vec![
            format!("let {}=0", VIM_SERVER_STATUS),
            format!("let {}='Rust: diagnostics end'", VIM_SERVER_STATUS_MESSAGE),
        ])?;
        Ok(())
    }
}
