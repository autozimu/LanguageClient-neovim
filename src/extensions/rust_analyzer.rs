use crate::{language_client::LanguageClient, types::WorkspaceEditWithCursor};
use anyhow::{anyhow, Result};
use jsonrpc_core::Value;
use lsp_types::{Command, Location};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CargoRunnable {
    pub workspace_root: Option<PathBuf>,
    pub cargo_args: Vec<String>,
    pub executable_args: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Runnable {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<lsp_types::LocationLink>,
    pub kind: RunnableKind,
    pub args: CargoRunnable,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum RunnableKind {
    Cargo,
}

pub mod command {
    pub const SHOW_REFERENCES: &str = "rust-analyzer.showReferences";
    pub const SELECT_APPLY_SOURCE_CHANGE: &str = "rust-analyzer.selectAndApplySourceChange";
    pub const APPLY_SOURCE_CHANGE: &str = "rust-analyzer.applySourceChange";
    pub const RUN_SINGLE: &str = "rust-analyzer.runSingle";
    pub const RUN: &str = "rust-analyzer.run";
}

impl LanguageClient {
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

                self.display_locations(&locations, "References")?;
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
                        let (bin, arguments) = match runnable.kind {
                            RunnableKind::Cargo => ("cargo", runnable.args.cargo_args),
                        };

                        let cmd = format!("term {} {}", bin, arguments.join(" "));
                        let cmd = cmd.replace('"', "");
                        self.vim()?.command(cmd)?;
                    }
                }
            }
            _ => return Ok(false),
        }

        Ok(true)
    }
}
