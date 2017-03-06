> If you are reporting a bug, please see following FAQ first.

# Not editor command: LanguageClientStart ?
- Run `:CheckHealth` to see if there is issue with neovim python3 host.
- Run `:UpdateRemotePlugins` and restart neovim to see if it helps.

# If you are still seeing an error or other types of error, please create ticket with
- Plugin version (git SHA)
- Detailed error message.
- Content of `~/.local/share/nvim/rplugin.vim`
- Run `:call LanguageClient_setLoggingLevel('DEBUG')` and then
  `:LanguageClientStart`, reproduce the bug, attach content of
  `/tmp/LanguageClient.log`.

  (Please understand the more detailed information you can provide, the
  quicker issue can be resolved.)
