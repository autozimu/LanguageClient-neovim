# LanguageClient-neovim
[Language Server Protocol](https://github.com/Microsoft/language-server-protocol) support for neovim.

![rename](https://raw.github.com/autozimu/images/master/LanguageClient-neovim/rename.gif)

# Quick Start

Using [`vim-plug`](https://github.com/junegunn/vim-plug):

```vimscript
Plug 'autozimu/LanguageClient-neovim'
Plug 'junegunn/fzf.vim'  " Optional dependency for symbol selection
Plug 'Shougo/deoplete.nvim'  " Optional dependency for completion
```

Example configuration

```vimscript
let g:LanguageClient_serverCommands = {
    \ 'rust': ['cargo', 'run', '--manifest-path=/opt/rls/Cargo.toml'],
    \ }

nnoremap <silent> K :call LanguageClient_textDocument_hover()<CR>
nnoremap <silent> gd :call LanguageClient_textDocument_definition()<CR>
nnoremap <silent> <F2> :call LanguageClient_textDocument_rename()<CR>
```

# Commands/Functions

- `LanguageClientStart`
- `LanguageClient_textDocument_hover()`
- `LanguageClient_textDocument_definition()`
- `LanguageClient_textDocument_rename()`
- `LanguageClient_textDocument_documentSymbol()`
- `LanguageClient_workspace_symbol()`
- Completion integration with deoplete.
