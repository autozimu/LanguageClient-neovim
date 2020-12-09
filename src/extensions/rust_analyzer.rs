use crate::types;
use crate::{language_client::LanguageClient, types::WorkspaceEditWithCursor, utils::ToUrl};
use anyhow::{anyhow, Result};
use jsonrpc_core::Value;
use lsp_types::{request::Request, Command, Location, Range, TextDocumentIdentifier};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Runnable wraps the two possible shapes of a runnable action from rust-analyzer. Old-ish versions
// of it will use BinRunnable, whereas the newer ones use CargoRunnable.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
enum Runnable {
    Bin(BinRunnable),
    Generic(GenericRunnable),
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct BinRunnable {
    pub label: String,
    pub bin: String,
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct GenericRunnable {
    pub label: String,
    pub kind: GenericRunnableKind,
    pub location: Option<lsp_types::LocationLink>,
    pub args: GenericRunnableArgs,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct GenericRunnableArgs {
    pub workspace_root: Option<PathBuf>,
    pub cargo_args: Vec<String>,
    pub executable_args: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum GenericRunnableKind {
    Cargo,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum InlayKind {
    TypeHint,
    ParameterHint,
    ChainingHint,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InlayHint {
    pub range: Range,
    pub kind: InlayKind,
    pub label: String,
}

impl Into<types::InlayHint> for InlayHint {
    fn into(self) -> types::InlayHint {
        types::InlayHint {
            range: self.range,
            label: self.label,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlayHintsParams {
    text_document: TextDocumentIdentifier,
}

pub mod command {
    pub const SHOW_REFERENCES: &str = "rust-analyzer.showReferences";
    pub const SELECT_APPLY_SOURCE_CHANGE: &str = "rust-analyzer.selectAndApplySourceChange";
    pub const APPLY_SOURCE_CHANGE: &str = "rust-analyzer.applySourceChange";
    pub const RUN_SINGLE: &str = "rust-analyzer.runSingle";
    pub const RUN: &str = "rust-analyzer.run";
}

pub mod request {
    pub enum InlayHintsRequest {}

    impl lsp_types::request::Request for InlayHintsRequest {
        type Params = super::InlayHintsParams;
        type Result = Vec<super::InlayHint>;
        const METHOD: &'static str = "rust-analyzer/inlayHints";
    }
}

const FILETYPE: &str = "rust";
pub const SERVER_NAME: &str = "rust-analyzer";

impl LanguageClient {
    pub fn rust_analyzer_inlay_hints(&self, filename: &str) -> Result<Vec<types::InlayHint>> {
        let inlay_hints_enabled = self.get_state(|state| {
            state
                .initialization_options
                .get(SERVER_NAME)
                .as_ref()
                .map(|opt| {
                    opt.pointer("/inlayHints/enable")
                        .unwrap_or(&Value::Bool(false))
                        == &Value::Bool(true)
                })
                .unwrap_or_default()
        })?;
        if !inlay_hints_enabled {
            return Ok(vec![]);
        }

        let result: Vec<InlayHint> = self.get_client(&Some(FILETYPE.into()))?.call(
            request::InlayHintsRequest::METHOD,
            InlayHintsParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_string().to_url()?,
                },
            },
        )?;

        // we are only able to display chaining hints at the moment, as we can't place virtual texts in
        // between words
        Ok(result
            .into_iter()
            .filter(|h| h.kind == InlayKind::ChainingHint)
            .map(InlayHint::into)
            .collect())
    }

    pub fn handle_rust_analyzer_command(&self, cmd: &Command) -> Result<bool> {
        match cmd.command.as_str() {
            command::SHOW_REFERENCES => {
                let locations = cmd
                    .arguments
                    .clone()
                    .unwrap_or_default()
                    .get(2)
                    .cloned()
                    .unwrap_or_else(|| Value::Array(vec![]));
                let locations: Vec<Location> = serde_json::from_value(locations)?;

                self.present_list("References", &locations)?;
            }
            command::SELECT_APPLY_SOURCE_CHANGE => {
                if let Some(ref edits) = cmd.arguments {
                    for edit in edits {
                        let workspace_edits = <Vec<WorkspaceEditWithCursor>>::deserialize(edit)?;
                        for edit in workspace_edits {
                            self.apply_workspace_edit(&edit.workspace_edit)?;
                            if let Some(cursor_position) = edit.cursor_position {
                                self.vim()?.cursor(
                                    cursor_position.position.line + 1,
                                    cursor_position.position.character + 1,
                                )?;
                            }
                        }
                    }
                }
            }
            command::APPLY_SOURCE_CHANGE => {
                if let Some(ref edits) = cmd.arguments {
                    for edit in edits {
                        let edit = WorkspaceEditWithCursor::deserialize(edit)?;
                        self.apply_workspace_edit(&edit.workspace_edit)?;
                        if let Some(cursor_position) = edit.cursor_position {
                            self.vim()?.cursor(
                                cursor_position.position.line + 1,
                                cursor_position.position.character + 1,
                            )?;
                        }
                    }
                }
            }
            command::RUN_SINGLE | command::RUN => {
                let has_term: i32 = self.vim()?.eval("exists(':terminal')")?;
                if has_term == 0 {
                    return Err(anyhow!("Terminal support is required for this action"));
                }

                if let Some(ref args) = cmd.arguments {
                    if let Some(args) = args.first() {
                        let runnable = Runnable::deserialize(args)?;
                        let cmd = match runnable {
                            Runnable::Bin(runnable) => {
                                format!("term {} {}", runnable.bin, runnable.args.join(" "))
                            }
                            Runnable::Generic(runnable) => format!(
                                "term cargo {} -- {}",
                                runnable.args.cargo_args.join(" "),
                                runnable.args.executable_args.join(" "),
                            ),
                        };

                        self.vim()?.command(cmd.replace('"', ""))?;
                    }
                }
            }
            _ => return Ok(false),
        }

        Ok(true)
    }
}

#[cfg(test)]
mod test {
    use super::Runnable;
    use super::*;
    use lsp_types::Command;
    use serde::Deserialize;

    #[test]
    fn test_deserialize_bin_runnable() {
        let cmd = r#"{
            "title":"Run",
            "command":"rust-analyzer.runSingle",
            "arguments": [
                {
                    "args":["run","--package","somepkg","--bin","somebin"],
                    "bin":"cargo",
                    "cwd":"/home/dev/somebin",
                    "extraArgs":[],
                    "label":"run binary"
                }
            ]
        }"#;

        let cmd: Command = serde_json::from_str(cmd).unwrap();
        let actual = Runnable::deserialize(cmd.arguments.unwrap().first().unwrap())
            .expect("failed deserializing bin runnable");
        let expected = Runnable::Bin(BinRunnable {
            label: "run binary".into(),
            bin: "cargo".into(),
            args: vec!["run", "--package", "somepkg", "--bin", "somebin"]
                .into_iter()
                .map(|it| it.into())
                .collect(),
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_deserialize_generic_runnable() {
        let cmd = r#"{
            "title":"▶︎ Run",
            "command":"rust-analyzer.runSingle",
            "arguments":[{
                "args":{
                    "cargoArgs":["run","--package","somepkg","--bin","somebin"],
                    "executableArgs":[],
                    "workspaceRoot":"/home/dev/test"
                },
                "kind":"cargo",
                "label":"run binary"
            }]
        }"#;

        let cmd: Command = serde_json::from_str(cmd).unwrap();
        let actual = Runnable::deserialize(cmd.arguments.unwrap().first().unwrap())
            .expect("failed deserializing cargo runnable");
        let expected = Runnable::Generic(GenericRunnable {
            label: "run binary".into(),
            kind: GenericRunnableKind::Cargo,
            location: None,
            args: GenericRunnableArgs {
                workspace_root: Some("/home/dev/test".into()),
                cargo_args: vec!["run", "--package", "somepkg", "--bin", "somebin"]
                    .into_iter()
                    .map(|it| it.into())
                    .collect(),
                executable_args: vec![],
            },
        });
        assert_eq!(actual, expected);
    }
}
