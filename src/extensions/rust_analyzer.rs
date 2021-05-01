use crate::{
    language_client::LanguageClient,
    types::{self, WorkspaceEditWithCursor},
    utils::ToUrl,
};
use anyhow::{anyhow, Result};
use core::iter;
use jsonrpc_core::Value;
use lsp_types::{request::Request, Command, Location, Range, TextDocumentIdentifier};
use regex::{Captures, Regex};
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum InlayKind {
    TypeHint,
    ChainingHint,
    ParameterHint,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InlayHint {
    pub range: Range,
    pub kind: InlayKind,
    pub label: String,
}

impl From<InlayHint> for types::InlayHint {
    fn from(hint: InlayHint) -> Self {
        types::InlayHint {
            range: hint.range,
            label: hint.label,
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
    // join hints which belong to the same hint kind
    fn join_inlay_hints(inlay_hints: &[&InlayHint], prefix: &str, postfix: &str) -> Vec<InlayHint> {
        let indices: Vec<usize> = iter::once(0)
            .chain(
                inlay_hints
                    .windows(2)
                    .enumerate()
                    .filter(|(_, pair)| pair[0].range.end.line != pair[1].range.end.line)
                    .map(|(i, _)| i + 1),
            )
            .collect();
        indices
            .windows(2)
            .map(|pair| &inlay_hints[pair[0]..pair[1]])
            .map(|hints| InlayHint {
                label: prefix.to_string()
                    + &hints
                        .iter()
                        .map(|hint| hint.label.as_str())
                        .collect::<Vec<&str>>()
                        .join(", ")
                    + postfix,
                range: hints[0].range,
                kind: hints[0].kind.clone(),
            })
            .collect()
    }

    fn join_hint_groups(inlay_hints: &[InlayHint], groups_sep: &str) -> Vec<types::InlayHint> {
        let indices: Vec<usize> = iter::once(0)
            .chain(
                inlay_hints
                    .windows(2)
                    .enumerate()
                    .filter(|(_, pair)| pair[0].range.end.line != pair[1].range.end.line)
                    .map(|(i, _)| i + 1),
            )
            .collect();
        indices
            .windows(2)
            .map(|pair| &inlay_hints[pair[0]..pair[1]])
            .map(|hints| types::InlayHint {
                label: hints
                    .iter()
                    .map(|hint| hint.label.as_str())
                    .collect::<Vec<&str>>()
                    .join(groups_sep),
                range: hints[0].range,
            })
            .collect()
    }

    fn get_init_option(&self, opt_pointer: &str) -> Option<Value> {
        self.get_state(|state| {
            state
                .initialization_options
                .get(SERVER_NAME)
                .and_then(|opt| opt.pointer(opt_pointer).cloned())
        })
        .unwrap()
    }

    fn get_bool_init_option(&self, opt_pointer: &str) -> Option<bool> {
        self.get_init_option(opt_pointer)
            .map(|f| f == Value::Bool(true))
    }

    fn get_inlay_hints(&self, filename: &str) -> Result<Vec<InlayHint>> {
        self.get_client(&Some(FILETYPE.into()))?.call(
            request::InlayHintsRequest::METHOD,
            InlayHintsParams {
                text_document: TextDocumentIdentifier {
                    uri: filename.to_string().to_url()?,
                },
            },
        )
    }

    fn shorten_chain_hint(chain_hint: &InlayHint) -> InlayHint {
        lazy_static! {
            static ref IMPL_ITERATOR_REGEX: Regex =
                Regex::new("impl Iterator<Item = (.*)>").unwrap();
        }
        InlayHint {
            label: IMPL_ITERATOR_REGEX
                .replace(&chain_hint.label, |caps: &Captures| {
                    format!("Iterator<{}>", &caps[1])
                })
                .to_string(),
            range: chain_hint.range,
            kind: chain_hint.kind.clone(),
        }
    }

    fn process_chain_hints(inlay_hints: &[InlayHint]) -> Vec<InlayHint> {
        let chain_hints: Vec<InlayHint> = inlay_hints
            .iter()
            .filter(|hint| hint.kind == InlayKind::ChainingHint)
            .map(Self::shorten_chain_hint)
            .collect();
        let mut chain_hint_refs: Vec<&InlayHint> = chain_hints.iter().collect();
        chain_hint_refs.sort_unstable_by_key(|h| (h.range.end.line, h.range.end.character));
        Self::join_inlay_hints(&chain_hint_refs, "", "")
    }

    fn process_type_hints(inlay_hints: &[InlayHint]) -> Vec<InlayHint> {
        let mut type_hints: Vec<&InlayHint> = inlay_hints
            .iter()
            .filter(|hint| hint.kind == InlayKind::TypeHint)
            .collect();
        type_hints.sort_unstable_by_key(|h| (h.range.end.line, h.range.end.character));
        Self::join_inlay_hints(&type_hints, ": ", "")
    }

    fn process_param_hints(inlay_hints: &[InlayHint]) -> Vec<InlayHint> {
        let mut param_hints: Vec<&InlayHint> = inlay_hints
            .iter()
            .filter(|hint| hint.kind == InlayKind::ParameterHint)
            .collect();
        param_hints.sort_unstable_by_key(|h| (h.range.end.line, h.range.end.character));
        Self::join_inlay_hints(&param_hints, "𝑓(", ")")
    }

    pub fn rust_analyzer_inlay_hints(&self, filename: &str) -> Result<Vec<types::InlayHint>> {
        if !self
            .get_bool_init_option("/inlayHints/enable")
            .unwrap_or(true)
        {
            return Ok(vec![]);
        }
        let is_chain_hints = self
            .get_bool_init_option("/inlayHints/chainingHints")
            .unwrap_or(true);
        let is_type_hints = self
            .get_bool_init_option("/inlayHints/typeHints")
            .unwrap_or(true);
        let is_param_hints = self
            .get_bool_init_option("/inlayHints/parameterHints")
            .unwrap_or(true);

        if !is_chain_hints && !is_type_hints && !is_param_hints {
            return Ok(vec![]);
        }

        let inlay_hints = self.get_inlay_hints(filename)?;
        let hint_groups_separator = " | ";

        // NeoVIM doesn't support displaying virtual text in the middle of the line.
        // As a workaround, we join all inlay hints located on the same line
        // into single type hint and display it at the end of the line.
        let mut semi_joined_hints = Vec::with_capacity(inlay_hints.len());
        if is_chain_hints {
            semi_joined_hints.extend(Self::process_chain_hints(&inlay_hints));
        }
        if is_type_hints {
            semi_joined_hints.extend(Self::process_type_hints(&inlay_hints));
        }
        if is_param_hints {
            semi_joined_hints.extend(Self::process_param_hints(&inlay_hints));
        }

        semi_joined_hints
            .sort_unstable_by_key(|h| (h.range.end.line, h.range.end.character, h.kind.clone()));
        Ok(Self::join_hint_groups(
            &semi_joined_hints,
            hint_groups_separator,
        ))
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
