use crate::{
    rpcclient::RpcClient,
    sign::Sign,
    types::{Bufnr, QuickfixEntry, VimExp, VirtualText},
    utils::Canonicalize,
    viewport::Viewport,
};
use anyhow::Result;
use jsonrpc_core::Value;
use log::*;
use lsp_types::Position;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use std::{path::Path, sync::Arc};

/// Try get value of an variable from RPC params.
pub fn try_get<'a, R: Deserialize<'a>>(key: &str, params: &'a Value) -> Result<Option<R>> {
    let value = &params[key];
    if value == &Value::Null {
        Ok(None)
    } else {
        Ok(<Option<R>>::deserialize(value)?)
    }
}

#[derive(Clone, Copy, Serialize)]
pub struct HighlightSource {
    pub buffer: Bufnr,
    pub source: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Highlight {
    pub line: u64,
    pub character_start: u64,
    pub character_end: u64,
    pub group: String,
    pub text: String,
}

#[derive(PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
    Command,
    Select,
    SelectLine,
    SelectBlock,
    Terminal,
}

impl From<&str> for Mode {
    fn from(mode: &str) -> Self {
        match mode {
            "n" => Mode::Normal,
            "i" => Mode::Insert,
            "R" => Mode::Replace,
            "v" => Mode::Visual,
            "V" => Mode::VisualLine,
            "<C-v>" => Mode::VisualBlock,
            "c" => Mode::Command,
            "s" => Mode::Select,
            "S" => Mode::SelectLine,
            "<C-s>" => Mode::SelectBlock,
            "t" => Mode::Terminal,
            m => {
                error!("unknown mode {}, falling back to Mode::Normal", m);
                Mode::Normal
            }
        }
    }
}

#[derive(Clone)]
pub struct Vim {
    pub rpcclient: Arc<RpcClient>,
}

impl Vim {
    pub fn new(rpcclient: Arc<RpcClient>) -> Self {
        Self { rpcclient }
    }

    /// Fundamental functions.

    pub fn get_mode(&self) -> Result<Mode> {
        let mode: String = self.rpcclient.call("mode", json!([]))?;
        Ok(Mode::from(mode.as_str()))
    }

    pub fn command(&self, cmds: impl Serialize) -> Result<()> {
        self.rpcclient.notify("s:command", &cmds)
    }

    pub fn eval<E, T>(&self, exp: E) -> Result<T>
    where
        E: VimExp,
        T: DeserializeOwned,
    {
        self.rpcclient.call("eval", &exp.to_exp())
    }

    /// Function wrappers.

    pub fn getbufvar<R: DeserializeOwned>(&self, bufname: &str, var: &str) -> Result<R> {
        self.rpcclient.call("getbufvar", json!([bufname, var]))
    }

    pub fn get_filename(&self, params: &Value) -> Result<String> {
        let key = "filename";
        let expr = "LSP#filename()";

        let filename: String = try_get(key, params)?.map_or_else(|| self.eval(expr), Ok)?;
        Ok(filename.canonicalize())
    }

    pub fn get_language_id(&self, filename: &str, params: &Value) -> Result<String> {
        let key = "languageId";
        let expr = "&filetype";

        try_get(key, params)?.map_or_else(|| self.getbufvar(filename, expr), Ok)
    }

    pub fn get_bufnr(&self, filename: &str, params: &Value) -> Result<Bufnr> {
        let key = "bufnr";

        try_get(key, params)?.map_or_else(|| self.eval(format!("bufnr('{}')", filename)), Ok)
    }

    pub fn get_viewport(&self, params: &Value) -> Result<Viewport> {
        let key = "viewport";
        let expr = "LSP#viewport()";

        try_get(key, params)?.map_or_else(|| self.eval(expr), Ok)
    }

    pub fn get_position(&self, params: &Value) -> Result<Position> {
        let key = "position";
        let expr = "LSP#position()";

        try_get(key, params)?.map_or_else(|| self.eval(expr), Ok)
    }

    pub fn get_current_word(&self, params: &Value) -> Result<String> {
        let key = "cword";
        let expr = "expand('<cword>')";

        try_get(key, params)?.map_or_else(|| self.eval(expr), Ok)
    }

    pub fn get_goto_cmd(&self, params: &Value) -> Result<Option<String>> {
        let key = "gotoCmd";

        try_get(key, params)
    }

    pub fn get_tab_size(&self) -> Result<u64> {
        let expr = "shiftwidth()";

        self.eval(expr)
    }

    pub fn get_insert_spaces(&self, filename: &str) -> Result<bool> {
        let insert_spaces: i8 = self.getbufvar(filename, "&expandtab")?;
        Ok(insert_spaces == 1)
    }

    pub fn get_text(&self, bufname: &str) -> Result<Vec<String>> {
        self.rpcclient.call("LSP#text", json!([bufname]))
    }

    pub fn get_handle(&self, params: &Value) -> Result<bool> {
        let key = "handle";

        try_get(key, params)?.map_or_else(|| Ok(true), Ok)
    }

    pub fn echo(&self, message: impl AsRef<str>) -> Result<()> {
        self.rpcclient.notify("s:Echo", message.as_ref())
    }

    pub fn echo_ellipsis(&self, message: impl AsRef<str>) -> Result<()> {
        let message = message.as_ref().lines().collect::<Vec<_>>().join(" ");
        self.rpcclient.notify("s:EchoEllipsis", message)
    }

    pub fn echomsg_ellipsis(&self, message: impl AsRef<str>) -> Result<()> {
        let message = message.as_ref().lines().collect::<Vec<_>>().join(" ");
        self.rpcclient.notify("s:EchomsgEllipsis", message)
    }

    pub fn echomsg(&self, message: impl AsRef<str>) -> Result<()> {
        self.rpcclient.notify("s:Echomsg", message.as_ref())
    }

    pub fn echoerr(&self, message: impl AsRef<str>) -> Result<()> {
        self.rpcclient.notify("s:Echoerr", message.as_ref())
    }

    pub fn echowarn(&self, message: impl AsRef<str>) -> Result<()> {
        self.rpcclient.notify("s:Echowarn", message.as_ref())
    }

    pub fn cursor(&self, lnum: u64, col: u64) -> Result<()> {
        self.rpcclient.notify("cursor", json!([lnum, col]))
    }

    #[allow(dead_code)]
    pub fn setline(&self, lnum: u64, text: &[String]) -> Result<()> {
        self.rpcclient.notify("setline", json!([lnum, text]))
    }

    pub fn edit(&self, goto_cmd: &Option<String>, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().to_string_lossy();
        let goto = goto_cmd.as_deref().unwrap_or("edit");
        self.rpcclient.notify("s:Edit", json!([goto, path]))?;
        Ok(())
    }

    pub fn setqflist(&self, list: &[QuickfixEntry], action: &str, title: &str) -> Result<()> {
        info!("Begin setqflist");
        let parms = json!([list, action]);
        self.rpcclient.notify("setqflist", parms)?;
        let parms = json!([[], "a", { "title": title }]);
        self.rpcclient.notify("setqflist", parms)?;
        Ok(())
    }

    pub fn setloclist(&self, list: &[QuickfixEntry], action: &str, title: &str) -> Result<()> {
        let parms = json!([0, list, action]);
        self.rpcclient.notify("setloclist", parms)?;
        let parms = json!([0, [], "a", { "title": title }]);
        self.rpcclient.notify("setloclist", parms)?;
        Ok(())
    }

    /// clears all highlights in the current buffer.
    pub fn clear_highlights(&self, namespace: &str) -> Result<()> {
        self.rpcclient
            .notify("s:ClearHighlights", json!([namespace]))
    }

    /// replaces the highlights of the current document with the passed highlights.
    pub fn set_highlights(&self, highlights: &[Highlight], namespace: &str) -> Result<()> {
        if highlights.is_empty() {
            return self.clear_highlights(namespace);
        }

        self.rpcclient
            .notify("s:SetHighlights", json!([highlights, namespace]))
    }

    pub fn create_namespace(&self, name: &str) -> Result<i64> {
        self.rpcclient.call("nvim_create_namespace", [name])
    }

    pub fn set_virtual_texts(
        &self,
        buf_id: i64,
        ns_id: i64,
        line_start: u64,
        line_end: u64,
        virtual_texts: &[VirtualText],
    ) -> Result<i8> {
        self.rpcclient.call(
            "s:set_virtual_texts",
            json!([buf_id, ns_id, line_start, line_end, virtual_texts]),
        )
    }

    pub fn set_signs(&self, filename: &str, signs: &[Sign]) -> Result<i8> {
        self.rpcclient.call("s:set_signs", json!([filename, signs]))
    }
}
