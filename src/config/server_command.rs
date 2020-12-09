use crate::utils::expand_json_path;
use jsonrpc_core::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerDetails {
    pub command: Vec<String>,
    pub name: String,
    pub initialization_options: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerCommand {
    Simple(Vec<String>),
    Detailed(ServerDetails),
}

impl ServerCommand {
    pub fn get_command(&self) -> &[String] {
        match self {
            ServerCommand::Simple(cmd) => &cmd,
            ServerCommand::Detailed(cmd) => &cmd.command,
        }
    }

    /// Returns the server name from a ServerCommand. For compatibility purposes, this
    /// makes a rather wild assumption when the server name hasn't been explicitly
    /// configured. The assumption is that the command for this server is an
    /// executable and that the name of the executable is the name of the server.
    /// This may not be true for many cases, but it's the best we can do to try and
    /// guess the name of the server.
    pub fn name(&self) -> String {
        match self {
            ServerCommand::Simple(cmd) => ServerCommand::name_from_command(&cmd),
            ServerCommand::Detailed(cmd) => cmd.name.clone(),
        }
    }

    pub fn initialization_options(&self) -> Value {
        match self {
            ServerCommand::Simple(_) => Value::Null,
            ServerCommand::Detailed(cmd) => {
                let options = cmd.initialization_options.clone();
                expand_json_path(options.unwrap_or_default())
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn name_from_command(cmd: &[String]) -> String {
        // it's safe to assume there is at least one item in cmd, otherwise
        // this would be an empty server command
        let first = cmd.first().cloned().unwrap_or_default();
        first.split('/').last().unwrap_or_default().to_string()
    }

    #[cfg(target_os = "windows")]
    fn name_from_command(cmd: &[String]) -> String {
        // it's safe to assume there is at least one item in cmd, otherwise
        // this would be an empty server command
        let first = cmd.first().cloned().unwrap_or_default();
        first.split('\\').last().unwrap_or_default().to_string()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_name_from_command_handles_binary_name() {
        let name = ServerCommand::name_from_command(&vec!["gopls".into()]);
        assert_eq!(name.as_str(), "gopls");
    }

    #[test]
    fn test_name_from_command_handles_binary_path() {
        let name = ServerCommand::name_from_command(&vec!["/path/to/gopls".into()]);
        assert_eq!(name.as_str(), "gopls");
    }
}
