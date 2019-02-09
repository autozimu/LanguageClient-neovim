> For legacy python implementation, see [branch master](https://github.com/autozimu/LanguageClient-neovim/tree/master).

# LanguageClient-neovim
[![CircleCI](https://circleci.com/gh/autozimu/LanguageClient-neovim.svg?style=svg)](https://circleci.com/gh/autozimu/LanguageClient-neovim)

[Language Server Protocol] (LSP) support for [vim] and [neovim].

[Language Server Protocol]: https://github.com/Microsoft/language-server-protocol
[neovim]: https://neovim.io/
[vim]: http://www.vim.org/

![rename](https://cloud.githubusercontent.com/assets/1453551/24251636/2e73a1cc-0fb1-11e7-8a5e-3332e6a5f424.gif)

More recordings at [Updates, screenshots & GIFs](https://github.com/autozimu/LanguageClient-neovim/issues/35).

# Features

- Non-blocking asynchronous calls.
- [Sensible completion](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731936).
  Integrated with [deoplete](https://github.com/Shougo/deoplete.nvim) or
  [ncm2](https://github.com/ncm2/ncm2). Or with vim built-in `omnifunc`.
- [Realtime diagnostics/compiler/lint message.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288732042)
- [Rename.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731403)
- [Hover/Get identifier info.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731665)
- [Goto definition.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731744)
- Goto reference locations.
- [Workspace/Document symbols query](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731839).
- [Formatting](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-324497559).
- [Code Action/Quick Fix](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-331016526).

# Quick Start

Using [`vim-plug`](https://github.com/junegunn/vim-plug):

```vim
Plug 'autozimu/LanguageClient-neovim', {
    \ 'branch': 'next',
    \ 'do': 'bash install.sh',
    \ }

" (Optional) Multi-entry selection UI.
Plug 'junegunn/fzf'

Plug 'Shougo/deoplete.nvim', { 'do': ':UpdateRemotePlugins' }
```

Example configuration

```vim
" Required for operations modifying multiple buffers like rename.
set hidden

let g:LanguageClient_serverCommands = {
    \ 'rust': ['~/.cargo/bin/rustup', 'run', 'stable', 'rls'],
    \ 'javascript': ['/usr/local/bin/javascript-typescript-stdio'],
    \ 'javascript.jsx': ['tcp://127.0.0.1:2089'],
    \ 'python': ['/usr/local/bin/pyls'],
    \ }

nnoremap <F5> :call LanguageClient_contextMenu()<CR>
" Or map each action separately
nnoremap <silent> K :call LanguageClient#textDocument_hover()<CR>
nnoremap <silent> gd :call LanguageClient#textDocument_definition()<CR>
nnoremap <silent> <F2> :call LanguageClient#textDocument_rename()<CR>
```

Run command `nvim +PlugInstall +UpdateRemotePlugins +qa` in shell to install
this plugin. Install corresponding language servers. Restart neovim/vim and
language services will be available right away. Happy hacking!

# Install

[INSTALL](INSTALL.md)

# Troubleshooting

[Troubleshooting](INSTALL.md#6-troubleshooting)

# Language Servers

Please see <http://langserver.org> and/or <https://microsoft.github.io/language-server-protocol/implementors/servers/>.

# Documentation

[`:help LanguageClient`][LanguageClient.txt] for full list of configurations, commands and functions.

[LanguageClient.txt]: doc/LanguageClient.txt

# Changelog
See [Updates]. Subscribe the issue if you want to receive notifications.

[Updates]: https://github.com/autozimu/LanguageClient-neovim/issues/35

# Contributing

[CONTRIBUTING](.github/CONTRIBUTING.md)

# License

The MIT License.
