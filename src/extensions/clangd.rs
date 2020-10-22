use crate::{language_client::LanguageClient, utils::ToUrl};
use anyhow::Result;
use jsonrpc_core::Value;
use lsp_types::{request::Request, TextDocumentIdentifier};

pub mod request {
    use lsp_types::{request::Request, TextDocumentIdentifier};

    pub enum SwitchSourceHeader {}

    impl Request for SwitchSourceHeader {
        type Params = TextDocumentIdentifier;
        type Result = String;
        const METHOD: &'static str = "textDocument/switchSourceHeader";
    }
}

impl LanguageClient {
    pub fn text_document_switch_source_header(&self, params: &Value) -> Result<Value> {
        let filename = self.vim()?.get_filename(params)?;
        let language_id = self.vim()?.get_language_id(&filename, &Value::Null)?;
        let params = TextDocumentIdentifier {
            uri: filename.to_url()?,
        };

        let response: String = self
            .get_client(&Some(language_id))?
            .call(request::SwitchSourceHeader::METHOD, params)?;

        let path = std::path::Path::new(&response);
        self.vim()?.edit(&None, path)?;

        Ok(Value::Null)
    }
}
