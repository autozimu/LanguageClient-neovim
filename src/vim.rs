use super::*;
use crate::rpcclient::RpcClient;
use crate::sign::Sign;
use crate::viewport::Viewport;

/// Try get value of an variable from RPC params.
pub fn try_get<R: DeserializeOwned>(key: &str, params: &Value) -> Fallible<Option<R>> {
    let value = &params[key];
    if value == &Value::Null {
        Ok(None)
    } else {
        Ok(serde_json::from_value(value.clone())?)
    }
}

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

    pub fn get_filename(&self, params: &Value) -> Fallible<String> {
        let key = "filename";
        let expr = "LSP#filename()";

        try_get(key, params)?.map_or_else(|| self.eval(expr), Ok)
    }

    pub fn get_languageId(&self, filename: &str, params: &Value) -> Fallible<String> {
        let key = "languageId";
        let expr = "&filetype";

        try_get(key, params)?.map_or_else(|| self.getbufvar(filename, expr), Ok)
    }

    pub fn get_bufnr(&self, filename: &str, params: &Value) -> Fallible<Bufnr> {
        let key = "bufnr";

        try_get(key, params)?.map_or_else(|| self.eval(format!("bufnr('{}')", filename)), Ok)
    }

    pub fn get_viewport(&self, params: &Value) -> Fallible<Viewport> {
        let key = "viewport";
        let expr = "LSP#viewport()";

        try_get(key, params)?.map_or_else(|| self.eval(expr), Ok)
    }

    pub fn get_position(&self, params: &Value) -> Fallible<Position> {
        let key = "position";
        let expr = "LSP#position()";

        try_get(key, params)?.map_or_else(|| self.eval(expr), Ok)
    }

    pub fn get_current_word(&self, params: &Value) -> Fallible<String> {
        let key = "cword";
        let expr = "expand('<cword>')";

        try_get(key, params)?.map_or_else(|| self.eval(expr), Ok)
    }

    pub fn get_goto_cmd(&self, params: &Value) -> Fallible<Option<String>> {
        let key = "gotoCmd";

        try_get(key, params)
    }

    pub fn get_tab_size(&self) -> Fallible<u64> {
        let expr = "shiftwidth()";

        self.eval(expr)
    }

    pub fn get_insert_spaces(&self, filename: &str) -> Fallible<bool> {
        let insert_spaces: i8 = self.getbufvar(filename, "&expandtab")?;
        Ok(insert_spaces == 1)
    }

    pub fn get_text(&self, bufname: &str) -> Fallible<Vec<String>> {
        self.rpcclient.call("LSP#text", json!([bufname]))
    }

    pub fn get_handle(&self, params: &Value) -> Fallible<bool> {
        let key = "handle";

        try_get(key, params)?.map_or_else(|| Ok(true), Ok)
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

    #[allow(dead_code)]
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
    ) -> Fallible<i8> {
        self.rpcclient.call(
            "s:set_virtual_texts",
            json!([buf_id, ns_id, line_start, line_end, virtual_texts]),
        )
    }

    pub fn set_signs(
        &self,
        filename: &str,
        signs_to_add: &Vec<Sign>,
        signs_to_delete: &Vec<Sign>,
    ) -> Fallible<i8> {
        self.rpcclient.call(
            "s:set_signs",
            json!([filename, signs_to_add, signs_to_delete]),
        )
    }
}
