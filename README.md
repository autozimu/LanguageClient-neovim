# LanguageClient-neovim
[![Build Status](https://travis-ci.org/autozimu/LanguageClient-neovim.svg?branch=next)](https://travis-ci.org/autozimu/LanguageClient-neovim)

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
Plug 'autozimu/LanguageClient-neovim', {
    \ 'branch': 'next',
    \ 'do': './install.sh'
    \ }

" (Optional) Multi-entry selection UI.
Plug 'junegunn/fzf'

" (Completion plugin option 1)
Plug 'roxma/nvim-completion-manager'
" (Completion plugin option 2)
Plug 'Shougo/deoplete.nvim', { 'do': ':UpdateRemotePlugins' }
```

Example configuration

```vim
" Required for operations modifying multiple buffers like rename.
set hidden

let g:LanguageClient_serverCommands = {
    \ 'rust': ['rustup', 'run', 'nightly', 'rls'],
    \ 'javascript': ['javascript-typescript-stdio'],
    \ }

nnoremap <silent> K :call LanguageClient_textDocument_hover()<CR>
nnoremap <silent> gd :call LanguageClient_textDocument_definition()<CR>
nnoremap <silent> <F2> :call LanguageClient_textDocument_rename()<CR>
```

Run command `nvim +PlugInstall +UpdateRemotePlugins +qa` in shell to install
this plugin. Install corresponding language servers. Restart neovim/vim and
language services will be available right away. Happy hacking!

Please see [INSTALL](INSTALL.md) for complete installation and configuration
instructions.

# Troubleshooting

[Troubleshooting](INSTALL.md#6-troubleshooting)

# Language Servers

Please see <http://langserver.org> and/or <https://github.com/Microsoft/language-server-protocol/wiki/Protocol-Implementations>.

# Documentation

See [LanguageClient.txt] for full list of configurations, commands and
functions.

[LanguageClient.txt]: doc/LanguageClient.txt

# Development

[DEVELOPMENT](DEVELOPMENT.md)

# License

The MIT License.
