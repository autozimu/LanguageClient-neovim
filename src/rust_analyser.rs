use crate::lsp::request::Request;
use crate::lsp::{Range, TextDocumentIdentifier};

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlayHintsParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
pub enum InlayKind {
    TypeHint,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
pub struct InlayHint {
    pub range: Range,
    pub kind: InlayKind,
    pub label: String,
}

#[derive(Debug)]
pub enum InlayHintRequest {}

impl Request for InlayHintRequest {
    type Params = InlayHintsParams;
    type Result = Option<Vec<InlayHint>>;
    const METHOD: &'static str = "rust-analyser/inlayHints";
}
