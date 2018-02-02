use super::*;

pub trait IVim {
    fn echo(&self, message: &str) -> Result<()>;
    fn echo_ellipsis(&self, message: &str) -> Result<()>;
    fn echomsg(&self, message: &str) -> Result<()>;
    fn echoerr(&self, message: &str) -> Result<()>;
    fn echowarn(&self, message: &str) -> Result<()>;
    fn eval<E, T>(&self, exp: E) -> Result<T>
    where
        E: VimExp,
        T: DeserializeOwned;
    fn command(&self, cmd: &str) -> Result<()>;
    fn getbufline<P: AsRef<Path>>(&self, bufexp: P) -> Result<Vec<String>>;
    fn goto_location<P: AsRef<Path>>(
        &self,
        goto_cmd: &Option<String>,
        path: P,
        line: u64,
        character: u64,
    ) -> Result<()>;
}

// Whether should it be Mutex or RwLock.
// Even though RwLock allows several readers at the same time, it won't bring too much good in this
// use case. As in this project, read and write are almost same amount of short operations. For
// RwLock to work, a writer still needs to wait for all readers finish their work before making the
// change.

impl IVim for Arc<Mutex<State>> {
    fn echo(&self, message: &str) -> Result<()> {
        let message = escape_single_quote(message);
        let cmd = format!("echo '{}'", message);
        self.command(cmd.as_str())
    }

    fn echo_ellipsis(&self, message: &str) -> Result<()> {
        let columns: usize = self.eval("&columns")?;
        let mut message = message.replace('\n', ". ");
        if message.len() > columns - 12 {
            message = message[..columns - 15].to_owned();
            message += "...";
        }
        self.echo(message.as_str())
    }

    fn echomsg(&self, message: &str) -> Result<()> {
        let message = escape_single_quote(message);
        let cmd = format!("echomsg '{}'", message);
        self.command(cmd.as_str())
    }

    fn echoerr(&self, message: &str) -> Result<()> {
        let message = escape_single_quote(message);
        let cmd = format!("echohl Error | echomsg '{}' | echohl None", message);
        self.command(cmd.as_str())
    }

    fn echowarn(&self, message: &str) -> Result<()> {
        let message = escape_single_quote(message);
        let cmd = format!("echohl WarningMsg | echomsg '{}' | echohl None", message);
        self.command(cmd.as_str())
    }

    fn eval<E, T>(&self, exp: E) -> Result<T>
    where
        E: VimExp,
        T: DeserializeOwned,
    {
        let result = self.call(None, "eval", exp.to_exp())?;
        Ok(serde_json::from_value(result)?)
    }

    fn command(&self, cmd: &str) -> Result<()> {
        self.notify(None, "execute", cmd)
    }

    fn getbufline<P: AsRef<Path>>(&self, bufexp: P) -> Result<Vec<String>> {
        let bufexp = bufexp.as_ref().to_string_lossy();
        let result = self.call(None, "getbufline", json!([bufexp, 1, '$']))?;
        Ok(serde_json::from_value(result)?)
    }

    fn goto_location<P: AsRef<Path>>(
        &self,
        goto_cmd: &Option<String>,
        path: P,
        line: u64,
        character: u64,
    ) -> Result<()> {
        let path = path.as_ref().to_string_lossy();
        let goto_cmd = if let Some(ref goto_cmd) = *goto_cmd {
            goto_cmd
        } else if self.eval::<_, i64>(format!("bufnr('{}')", path))? != -1 {
            "buffer"
        } else {
            "edit"
        };
        let cmd = format!(
            "echo | execute '{} +:call\\ cursor({},{}) ' . fnameescape('{}')",
            goto_cmd,
            line + 1,
            character + 1,
            path
        );

        self.command(cmd.as_str())
    }
}
