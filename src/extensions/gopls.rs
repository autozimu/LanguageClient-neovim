use crate::language_client::LanguageClient;
use anyhow::Result;
use lsp_types::Command;
use serde::Deserialize;
use serde_json::Value;

pub mod command {
    // older versions of gopls send code lens commands without the `gopls` prefix
    // so probably a good idea to maintain both for a while.
    pub(super) const TEST: &str = "test";
    pub(super) const GOPLS_TEST: &str = "gopls.test";
}

impl LanguageClient {
    pub fn handle_gopls_command(&self, cmd: &Command) -> Result<bool> {
        match cmd.command.as_str() {
            command::TEST | command::GOPLS_TEST => {
                if let Some(args) = &cmd.arguments {
                    if let Some(file) = args.get(0) {
                        let file = String::deserialize(file)?;
                        let path = parse_package_path(file.as_str()).unwrap_or("./...".into());
                        let run = <Option<Vec<String>>>::deserialize(
                            args.get(1).unwrap_or(&Value::Null),
                        )?;

                        let bench = <Option<Vec<String>>>::deserialize(
                            args.get(2).unwrap_or(&Value::Null),
                        )?;

                        let run = run.unwrap_or_default();
                        let bench = bench.unwrap_or_default();

                        if run.len() > 0 {
                            let cmd = format!("term go test -run '{}' {}", run.join("|"), path);
                            self.vim()?.command(cmd)?;
                        } else if bench.len() > 0 {
                            let cmd = format!("term go test -bench '{}' {}", bench.join("|"), path);
                            self.vim()?.command(cmd)?;
                        } else {
                            self.vim()?.echoerr("No tests to run")?;
                        }
                    }
                }
            }
            _ => return Ok(false),
        }

        Ok(true)
    }
}

fn parse_package_path(path: &str) -> Option<String> {
    let path = if path.starts_with("file://") {
        path.strip_prefix("file://")?
    } else {
        path
    };
    let path = std::path::PathBuf::from(path);
    Some(path.parent()?.to_str()?.to_owned())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_package_path() {
        let folder = parse_package_path("file:///home/dev/someone/project/file.go");
        assert!(folder.is_some());
        assert_eq!("/home/dev/someone/project", folder.unwrap());

        let folder = parse_package_path("/home/dev/someone/project/file.go");
        assert!(folder.is_some());
        assert_eq!("/home/dev/someone/project", folder.unwrap());
    }
}
