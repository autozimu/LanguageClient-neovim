call plug#begin('~/.local/share/nvim/plugged')

Plug 'autozimu/LanguageClient-neovim', { 'do': ':UpdateRemotePlugins' }

call plug#end()

autocmd BufReadPost *.rs setlocal filetype=rust

let g:LanguageClient_serverCommands = {
    \ 'rust': ['cargo', 'run', '--release', '--manifest-path=/opt/rls/Cargo.toml'],
    \ }
