use crate::language_client::LanguageClient;
use anyhow::Result;
use lsp_types::{Command, WorkspaceEdit};

pub mod command {
    pub const APPLY_WORKSPACE_EDIT: &str = "java.apply.workspaceEdit";
}

impl LanguageClient {
    pub fn handle_java_command(&self, cmd: &Command) -> Result<bool> {
        match cmd.command.as_str() {
            command::APPLY_WORKSPACE_EDIT => {
                if let Some(ref edits) = cmd.arguments {
                    for edit in edits {
                        let edit: WorkspaceEdit = serde_json::from_value(edit.clone())?;
                        self.apply_workspace_edit(&edit)?;
                    }
                }
            }

            _ => return Ok(false),
        }

        Ok(true)
    }
}
