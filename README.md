> This branch has been deprecated. Please use branch [next] if possible.

[next]: https://github.com/autozimu/LanguageClient-neovim/tree/next

# LanguageClient-neovim
[![Build Status](https://travis-ci.org/autozimu/LanguageClient-neovim.svg?branch=master)](https://travis-ci.org/autozimu/LanguageClient-neovim)

[Language Server Protocol] support for [neovim] and [vim].

[Language Server Protocol]: https://github.com/Microsoft/language-server-protocol
[neovim]: https://neovim.io/
[vim]: http://www.vim.org/

![rename](https://cloud.githubusercontent.com/assets/1453551/24251636/2e73a1cc-0fb1-11e7-8a5e-3332e6a5f424.gif)

More recordings at [Updates, screenshots & GIFs](https://github.com/autozimu/LanguageClient-neovim/issues/35).

# Features

- Non-blocking asynchronous calls.
- [Sensible completion](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731936).
  Integrated with [deoplete](https://github.com/Shougo/deoplete.nvim) 
  or [nvim-completion-manager](https://github.com/roxma/nvim-completion-manager).
- [Realtime diagnostics/compiler/lint message.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288732042)
- [Rename.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731403)
- [Get identifer info.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731665)
- [Goto definition.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731744)
- Goto reference locations.
- [Workspace/Document symbols query](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731839).
- [Formatting](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-324497559).
- [Code Action/Fix](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-331016526).

(Note: Most of the functionality are provided by language servers. Specific
language servers may implement only a subset of the features, see
<http://langserver.org>, in which case, featured listed above may not fully
functional.)

# Quick Start

Using [`vim-plug`](https://github.com/junegunn/vim-plug):

```vim
Plug 'autozimu/LanguageClient-neovim', { 'do': ':UpdateRemotePlugins' }

" (Optional) Multi-entry selection UI.
Plug 'junegunn/fzf'
" (Optional) Multi-entry selection UI.
Plug 'Shougo/denite.nvim'

" (Optional) Completion integration with deoplete.
Plug 'Shougo/deoplete.nvim', { 'do': ':UpdateRemotePlugins' }
" (Optional) Completion integration with nvim-completion-manager.
Plug 'roxma/nvim-completion-manager'

" (Optional) Showing function signature and inline doc.
Plug 'Shougo/echodoc.vim'
```

Example configuration

```vim
" Required for operations modifying multiple buffers like rename.
set hidden

let g:LanguageClient_serverCommands = {
    \ 'rust': ['rustup', 'run', 'nightly', 'rls'],
    \ 'javascript': ['/opt/javascript-typescript-langserver/lib/language-server-stdio.js'],
    \ }

" Automatically start language servers.
let g:LanguageClient_autoStart = 1

nnoremap <silent> K :call LanguageClient_textDocument_hover()<CR>
nnoremap <silent> gd :call LanguageClient_textDocument_definition()<CR>
nnoremap <silent> <F2> :call LanguageClient_textDocument_rename()<CR>
```

Run command `nvim +PlugInstall +UpdateRemotePlugins +qa` in shell to install
this plugin. Install corresponding language servers. Start neovim and language
services will be available right away if things go well. Happy hacking!

Please see [INSTALL](INSTALL.md) for complete installation and configuration
instructions.

# Troubleshooting

[Troubleshooting](https://github.com/autozimu/LanguageClient-neovim/blob/master/INSTALL.md#7-troubleshooting)

# Language Servers

Please see <http://langserver.org> and/or <https://github.com/Microsoft/language-server-protocol/wiki/Protocol-Implementations>.

# Documentation

See [LanguageClient.txt] for full list of configurations, commands and
functions.

[LanguageClient.txt]: https://github.com/autozimu/LanguageClient-neovim/blob/master/doc/LanguageClient.txt

# Development

[DEVELOPMENT](https://github.com/autozimu/LanguageClient-neovim/blob/master/DEVELOPMENT.md)

# License

The MIT License.
