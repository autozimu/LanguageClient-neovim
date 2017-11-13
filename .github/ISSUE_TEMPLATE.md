> If you are reporting a bug, please read following FAQ first.

# Not editor command: LanguageClientStart?
Have you followed the instructions at
[INSTALL.md](https://github.com/autozimu/LanguageClient-neovim/blob/master/INSTALL.md)?

If not, please try follow the instructions there first.

# If you are still seeing an error or other types of issue, please create ticket with
- neovim version
- Plugin version (git SHA)
- Minimal vimrc (e.g. <https://github.com/autozimu/LanguageClient-neovim/blob/master/min-init.vim>).
- Content of `:CheckHealth`.
- Content of `~/.local/share/nvim/rplugin.vim`.
- Language server name and version.
- Reproduction steps (from clean state).
- Run `:call LanguageClient_setLoggingLevel('DEBUG')` and then
  `:LanguageClientStart`, reproduce the bug, attach contents of
  `/tmp/LanguageClient.log` and `/tmp/LanguageServer.log`.

(Please understand the more detailed information provided, the sooner a issue
can be resolved. Low quality bug reports might be closed directly.)
