# LanguageClient-neovim
[![Build Status](https://travis-ci.org/autozimu/LanguageClient-neovim.svg?branch=master)](https://travis-ci.org/autozimu/LanguageClient-neovim)

[Language Server Protocol](https://github.com/Microsoft/language-server-protocol) support for [neovim](https://github.com/neovim/neovim).

![rename](https://raw.github.com/autozimu/images/master/LanguageClient-neovim/rename.gif)

More recordings at <https://github.com/autozimu/images/tree/master/LanguageClient-neovim>.

# Features

- Non-blocking asynchronous calls.
- [Sensible completion](https://github.com/autozimu/images/tree/master/LanguageClient-neovim#completion).
  Integrated with [deoplete](https://github.com/Shougo/deoplete.nvim) 
  or [nvim-completion-manager](https://github.com/roxma/nvim-completion-manager).
- [Realtime diagnostics/compiler/lint message.](https://github.com/autozimu/images/tree/master/LanguageClient-neovim#diagnostics)
- [Rename.](https://github.com/autozimu/images/tree/master/LanguageClient-neovim#rename)
- [Get identifer info.](https://github.com/autozimu/images/tree/master/LanguageClient-neovim#hover)
- [Goto definition.](https://github.com/autozimu/images/tree/master/LanguageClient-neovim#goto-definition)
- Goto reference locations.
- [Workspace/Document symbols query](https://github.com/autozimu/images/tree/master/LanguageClient-neovim#symbols).

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
    \ 'rust': ['cargo', 'run', '--release', '--manifest-path=/opt/rls/Cargo.toml'],
    \ }

nnoremap <silent> K :call LanguageClient_textDocument_hover()<CR>
nnoremap <silent> gd :call LanguageClient_textDocument_definition()<CR>
nnoremap <silent> <F2> :call LanguageClient_textDocument_rename()<CR>
```

Run command `LanguageClientStart` inside neovim to get start.

# Language Servers

Please see <http://langserver.org>.

# Documentation

[LanguageClient.txt](https://github.com/autozimu/LanguageClient-neovim/blob/master/doc/LanguageClient.txt)
