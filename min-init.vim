call plug#begin('~/.local/share/nvim/plugged')

Plug 'autozimu/LanguageClient-neovim', { 'branch': 'next', 'do': './install.sh' }

call plug#end()

autocmd BufReadPost *.rs setlocal filetype=rust

let g:LanguageClient_serverCommands = {
    \ 'rust': ['rustup', 'run', 'nightly', 'rls'],
    \ }
let g:LanguageClient_loggingLevel = 'DEBUG'
