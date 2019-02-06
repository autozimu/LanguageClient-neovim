use crate::viewport::Viewport;
use crate::vim::Vim;
use failure::Fallible;
use lazycell::LazyCell;
use lsp_types::Position;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;

pub struct Context {
    vim: Vim,
    bufname: String,
    language_id: LazyCell<String>,
    viewport: LazyCell<Viewport>,
    position: LazyCell<Position>,
    current_word: LazyCell<String>,
    text: LazyCell<Vec<String>>,
    prepopulated: HashMap<String, Value>,
}

impl Context {
    pub fn new(bufname: String, vim: Vim) -> Self {
        Context {
            vim,
            bufname,
            language_id: LazyCell::new(),
            viewport: LazyCell::new(),
            position: LazyCell::new(),
            current_word: LazyCell::new(),
            text: LazyCell::new(),
            prepopulated: HashMap::new(),
        }
    }

    /// Try get result of expression from prepopulated.
    fn try_get<R: DeserializeOwned>(&self, expr: &str) -> Fallible<Option<R>> {
        if let Some(value) = self.prepopulated.get(expr) {
            Ok(Some(serde_json::from_value(value.clone())?))
        } else {
            Ok(None)
        }
    }

    pub fn get_languageId(&self) -> Fallible<&String> {
        let expr = "&filetype";

        self.language_id.try_borrow_with(|| {
            self.try_get(expr)?
                .map_or_else(|| self.vim.getbufvar(&self.bufname, expr), Ok)
        })
    }

    pub fn get_viewport(&self) -> Fallible<&Viewport> {
        let expr = "LSP#viewport()";

        self.viewport
            .try_borrow_with(|| self.try_get(expr)?.map_or_else(|| self.vim.eval(expr), Ok))
    }

    pub fn get_position(&self) -> Fallible<&Position> {
        let expr = "LSP#position()";

        self.position
            .try_borrow_with(|| self.try_get(expr)?.map_or_else(|| self.vim.eval(expr), Ok))
    }

    pub fn get_current_word(&self) -> Fallible<&String> {
        let expr = "expand('<cword>')";

        self.current_word
            .try_borrow_with(|| self.try_get(expr)?.map_or_else(|| self.vim.eval(expr), Ok))
    }

    pub fn get_text(&self, start: &str, end: &str) -> Fallible<&Vec<String>> {
        self.text
            .try_borrow_with(|| self.vim.getbufline(&self.bufname, start, end))
    }
}
