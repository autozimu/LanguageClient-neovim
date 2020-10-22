pub mod clangd;
pub mod gopls;
pub mod java;
pub mod rust_analyzer;

use crate::language_client::LanguageClient;
use anyhow::Result;

impl LanguageClient {
    pub fn text_document_inlay_hints(&self, language_id: &str, filename: &str) -> Result<()> {
        if !self.extensions_enabled(language_id)? {
            return Ok(());
        }

        let server_name = self.get_state(|state| match state.capabilities.get(language_id) {
            Some(c) => c
                .server_info
                .as_ref()
                .map(|info| info.name.clone())
                .unwrap_or_default(),
            None => String::new(),
        })?;

        let hints = match server_name.as_str() {
            rust_analyzer::SERVER_NAME => self.rust_analyzer_inlay_hints(filename)?,
            _ => return Ok(()),
        };

        self.update_state(|state| {
            state.inlay_hints.insert(filename.to_string(), hints);
            Ok(())
        })?;

        Ok(())
    }

    pub fn extensions_enabled(&self, filetype: &str) -> Result<bool> {
        let result = self.get_config(|c| match &c.enable_extensions {
            Some(extensions) => extensions.get(filetype).cloned().unwrap_or(true),
            None => true,
        })?;
        Ok(result)
    }
}
