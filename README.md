# LanguageClient-neovim
[![Build Status](https://travis-ci.org/autozimu/LanguageClient-neovim.svg?branch=master)](https://travis-ci.org/autozimu/LanguageClient-neovim)

[Language Server Protocol](https://github.com/Microsoft/language-server-protocol) support for [neovim](https://github.com/neovim/neovim).

![rename](https://raw.github.com/autozimu/images/master/LanguageClient-neovim/rename.gif)

More recordings at <https://github.com/autozimu/images/tree/master/LanguageClient-neovim>.

# Features

- Non-blocking asynchronous calls.
- Full integration tests.

# Quick Start

Using [`vim-plug`](https://github.com/junegunn/vim-plug):

```vim
Plug 'autozimu/LanguageClient-neovim', { 'do': ':UpdateRemotePlugins' }
" Optional dependency for symbol selection
Plug 'junegunn/fzf'
" Optional dependency for completion
Plug 'Shougo/deoplete.nvim', { 'do': ':UpdateRemotePlugins' }
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

# Commands/Functions

- `LanguageClientStart`
- `LanguageClient_textDocument_hover()`
- `LanguageClient_textDocument_definition()`
- `LanguageClient_textDocument_rename()`
- `LanguageClient_textDocument_documentSymbol()`
- `LanguageClient_textDocument_references()`
- `LanguageClient_workspace_symbol()`
- Completion integration with deoplete.
