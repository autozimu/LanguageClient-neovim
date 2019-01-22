use super::*;
use crate::language_client::LanguageClient;
use crate::rpcclient::RpcClient;

impl LanguageClient {
    /////// Vim wrappers ///////

    pub fn vim(&self) -> Fallible<RpcClient> {
        self.get_client(&None)
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn eval<E, T>(&self, exp: E) -> Fallible<T>
    where
        E: VimExp,
        T: DeserializeOwned,
    {
        self.vim()?.call("eval", &exp.to_exp())
    }

    pub fn command(&self, cmds: impl Serialize) -> Fallible<()> {
        self.vim()?.notify("execute", &cmds)
    }

    ////// Vim builtin function wrappers ///////

    pub fn echo(&self, message: impl AsRef<str>) -> Fallible<()> {
        self.vim()?.notify("s:Echo", message.as_ref())
    }

    pub fn echo_ellipsis(&self, message: impl AsRef<str>) -> Fallible<()> {
        let message = message.as_ref().lines().collect::<Vec<_>>().join(" ");
        self.vim()?.notify("s:EchoEllipsis", message)
    }

    pub fn echomsg_ellipsis(&self, message: impl AsRef<str>) -> Fallible<()> {
        let message = message.as_ref().lines().collect::<Vec<_>>().join(" ");
        self.vim()?.notify("s:EchomsgEllipsis", message)
    }

    pub fn echomsg(&self, message: impl AsRef<str>) -> Fallible<()> {
        self.vim()?.notify("s:Echomsg", message.as_ref())
    }

    pub fn echoerr(&self, message: impl AsRef<str>) -> Fallible<()> {
        self.vim()?.notify("s:Echoerr", message.as_ref())
    }

    pub fn echowarn(&self, message: impl AsRef<str>) -> Fallible<()> {
        self.vim()?.notify("s:Echowarn", message.as_ref())
    }

    pub fn cursor(&self, lnum: u64, col: u64) -> Fallible<()> {
        self.vim()?.notify("cursor", json!([lnum, col]))
    }

    pub fn setline(&self, lnum: u64, text: &[String]) -> Fallible<()> {
        self.vim()?.notify("setline", json!([lnum, text]))
    }

    pub fn edit(&self, goto_cmd: &Option<String>, path: impl AsRef<Path>) -> Fallible<()> {
        let path = path.as_ref().to_string_lossy();

        let goto = goto_cmd.as_deref().unwrap_or("edit");
        self.vim()?.notify("s:Edit", json!([goto, path]))?;

        if path.starts_with("jdt://") {
            self.command("setlocal buftype=nofile filetype=java noswapfile")?;

            let result = self.java_classFileContents(&json!({
                VimVar::LanguageId.to_key(): "java",
                "uri": path,
            }))?;
            let content = match result {
                Value::String(s) => s,
                _ => bail!("Unexpected type: {:?}", result),
            };
            let lines: Vec<String> = content
                .lines()
                .map(std::string::ToString::to_string)
                .collect();
            self.setline(1, &lines)?;
        }
        Ok(())
    }

    pub fn setqflist(&self, list: &[QuickfixEntry], action: &str, title: &str) -> Fallible<()> {
        info!("Begin setqflist");
        let parms = json!([list, action]);
        self.vim()?.notify("setqflist", parms)?;
        let parms = json!([[], "a", { "title": title }]);
        self.vim()?.notify("setqflist", parms)?;
        Ok(())
    }

    pub fn setloclist(&self, list: &[QuickfixEntry], action: &str, title: &str) -> Fallible<()> {
        let parms = json!([0, list, action]);
        self.vim()?.notify("setloclist", parms)?;
        let parms = json!([0, [], "a", { "title": title }]);
        self.vim()?.notify("setloclist", parms)?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawMessage {
    Notification(rpc::Notification),
    MethodCall(rpc::MethodCall),
    Output(rpc::Output),
}
