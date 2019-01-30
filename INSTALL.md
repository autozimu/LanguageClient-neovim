# 1. Requirements
- neovim or
- vim >= 8.0

# 2. Install dependencies

None.

# 3. Install this plugin

> For Windows users, replace `bash install.sh` with `powershell -executionpolicy bypass -File install.ps1` in following
> snippets.

> If you don't want to use pre-built binaries, specify branch `next` and `make
> release` as post action after plugin installation and update. e.g., `Plug
> 'autozimu/LanguageClient-neovim', {'branch': 'next', 'do': 'make release'}`.

> Android is not supported using the `install.sh` script.

Choose steps matching your plugin manager.

## [vim-plug](https://github.com/junegunn/vim-plug) user
Add following to vimrc
```vim
Plug 'autozimu/LanguageClient-neovim', {
    \ 'branch': 'next',
    \ 'do': 'bash install.sh',
    \ }
```

Restart neovim and run `:PlugInstall` to install.

## [dein.vim](https://github.com/Shougo/dein.vim) user
Add following to vimrc
```vim
call dein#add('autozimu/LanguageClient-neovim', {
    \ 'rev': 'next',
    \ 'build': 'bash install.sh',
    \ })
```

Restart neovim and run `:call dein#install()` to install.

## Manual
Clone this repo into some place, e.g., `~/.vim-plugins`
```sh
mkdir -p ~/.vim-plugins
cd ~/.vim-plugins
git clone --depth 1 https://github.com/autozimu/LanguageClient-neovim.git
cd LanguageClient-neovim
bash install.sh
```

Add this plugin to vim/neovim `runtimepath`,
```vim
set runtimepath+=~/.vim-plugins/LanguageClient-neovim
```

# 4. Install language servers
Install language servers if corresponding language servers are not available
yet on your system. Please see <http://langserver.org> and/or
<https://github.com/Microsoft/language-server-protocol/wiki/Protocol-Implementations>
for list of language servers.

# 5. Configure this plugin
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

nnoremap <silent> K :call LanguageClient#textDocument_hover()<CR>
nnoremap <silent> gd :call LanguageClient#textDocument_definition()<CR>
nnoremap <silent> <F2> :call LanguageClient#textDocument_rename()<CR>
```

# 6. Troubleshooting

1. Backup your vimrc and use [min-vimrc.vim](min-vimrc.vim) as vimrc.
1. Try on [sample projects](tests/data).
1. Execute `:echo &runtimepath` and make sure the plugin path is in the list.
1. Make sure language server could be started when invoked manually from shell.
 Also try use absolute path for server commands, as PATH in vim might be different from shell env, especially on macOS.
1. Check content of log file. Also worth noting language server might have
   separate log file.
