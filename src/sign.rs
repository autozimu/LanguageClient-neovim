use lsp_types::{Diagnostic, DiagnosticSeverity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sign {
    pub id: u64,
    /// line number. 0-based.
    pub line: u64,
    pub name: String,
}

impl From<&Diagnostic> for Sign {
    fn from(diagnostic: &Diagnostic) -> Self {
        let line = diagnostic.range.start.line;
        let severity = diagnostic.severity.unwrap_or(DiagnosticSeverity::Hint);
        let name = format!("LanguageClient{:?}", severity);
        let id = 75_000 + line * DiagnosticSeverity::Hint as u64 + severity as u64;

        Sign { id, line, name }
    }
}
