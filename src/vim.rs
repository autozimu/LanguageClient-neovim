use super::*;
use crate::rpcclient::RpcClient;

#[derive(Clone, Serialize)]
pub struct Vim {
    pub rpcclient: RpcClient,
}

impl Vim {
    pub fn new(rpcclient: RpcClient) -> Self {
        Vim { rpcclient }
    }

    /// Fundamental functions.

    pub fn command(&self, cmds: impl Serialize) -> Fallible<()> {
        self.rpcclient.notify("s:command", &cmds)
    }

    pub fn eval<E, T>(&self, exp: E) -> Fallible<T>
    where
        E: VimExp,
        T: DeserializeOwned,
    {
        self.rpcclient.call("eval", &exp.to_exp())
    }

    /// Function wrappers.

    pub fn getbufvar<R: DeserializeOwned>(&self, bufname: &str, var: &str) -> Fallible<R> {
        self.rpcclient.call("getbufvar", json!([bufname, var]))
    }

    pub fn getbufline(&self, bufname: &str, start: &str, end: &str) -> Fallible<Vec<String>> {
        self.rpcclient
            .call("getbufline", json!([bufname, start, end]))
    }

    pub fn echo(&self, message: impl AsRef<str>) -> Fallible<()> {
        self.rpcclient.notify("s:Echo", message.as_ref())
    }

    pub fn echo_ellipsis(&self, message: impl AsRef<str>) -> Fallible<()> {
        let message = message.as_ref().lines().collect::<Vec<_>>().join(" ");
        self.rpcclient.notify("s:EchoEllipsis", message)
    }

    pub fn echomsg_ellipsis(&self, message: impl AsRef<str>) -> Fallible<()> {
        let message = message.as_ref().lines().collect::<Vec<_>>().join(" ");
        self.rpcclient.notify("s:EchomsgEllipsis", message)
    }

    pub fn echomsg(&self, message: impl AsRef<str>) -> Fallible<()> {
        self.rpcclient.notify("s:Echomsg", message.as_ref())
    }

    pub fn echoerr(&self, message: impl AsRef<str>) -> Fallible<()> {
        self.rpcclient.notify("s:Echoerr", message.as_ref())
    }

    pub fn echowarn(&self, message: impl AsRef<str>) -> Fallible<()> {
        self.rpcclient.notify("s:Echowarn", message.as_ref())
    }

    pub fn cursor(&self, lnum: u64, col: u64) -> Fallible<()> {
        self.rpcclient.notify("cursor", json!([lnum, col]))
    }

    pub fn setline(&self, lnum: u64, text: &[String]) -> Fallible<()> {
        self.rpcclient.notify("setline", json!([lnum, text]))
    }

    pub fn edit(&self, goto_cmd: &Option<String>, path: impl AsRef<Path>) -> Fallible<()> {
        let path = path.as_ref().to_string_lossy();

        let goto = goto_cmd.as_deref().unwrap_or("edit");
        self.rpcclient.notify("s:Edit", json!([goto, path]))?;

        if path.starts_with("jdt://") {
            self.command("setlocal buftype=nofile filetype=java noswapfile")?;

            // TODO
            // let result = self.java_classFileContents(&json!({
            //     VimVar::LanguageId.to_key(): "java",
            //     "uri": path,
            // }))?;
            // let content = match result {
            //     Value::String(s) => s,
            //     _ => bail!("Unexpected type: {:?}", result),
            // };
            // let lines: Vec<String> = content
            //     .lines()
            //     .map(std::string::ToString::to_string)
            //     .collect();
            // self.setline(1, &lines)?;
        }
        Ok(())
    }

    pub fn setqflist(&self, list: &[QuickfixEntry], action: &str, title: &str) -> Fallible<()> {
        info!("Begin setqflist");
        let parms = json!([list, action]);
        self.rpcclient.notify("setqflist", parms)?;
        let parms = json!([[], "a", { "title": title }]);
        self.rpcclient.notify("setqflist", parms)?;
        Ok(())
    }

    pub fn setloclist(&self, list: &[QuickfixEntry], action: &str, title: &str) -> Fallible<()> {
        let parms = json!([0, list, action]);
        self.rpcclient.notify("setloclist", parms)?;
        let parms = json!([0, [], "a", { "title": title }]);
        self.rpcclient.notify("setloclist", parms)?;
        Ok(())
    }

    pub fn create_namespace(&self, name: &str) -> Fallible<i64> {
        self.rpcclient.call("nvim_create_namespace", [name])
    }

    pub fn set_virtual_texts(
        &self,
        buf_id: i64,
        ns_id: i64,
        line_start: u64,
        line_end: u64,
        virtual_texts: &[VirtualText],
    ) -> Fallible<i64> {
        self.rpcclient.call(
            "s:set_virtual_texts",
            json!([buf_id, ns_id, line_start, line_end, virtual_texts]),
        )
    }
}

// TODO: move to types.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawMessage {
    Notification(rpc::Notification),
    MethodCall(rpc::MethodCall),
    Output(rpc::Output),
}
