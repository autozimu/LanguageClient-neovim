# Requirements

- neovim or
- vim >= 8.0

# Install dependencies

None.

# Install this plugin

## Linux

Packages are available for the following distributions:

### Arch Linux

There are two packages available on the [AUR][archlinux/aur]

- [`aur/languageclient-neovim`][archlinux/install/aur]
- [`aur/languageclient-neovim-bin`][archlinux/install/aur-bin]

[archlinux/aur]: https://wiki.archlinux.org/index.php/Arch_User_Repository
[archlinux/install/aur]: https://aur.archlinux.org/packages/languageclient-neovim
[archlinux/install/aur-bin]: https://aur.archlinux.org/packages/languageclient-neovim-bin


## With a plugin manager

> For Windows users, replace `bash install.sh` with `powershell -executionpolicy bypass -File install.ps1` in following
> snippets.

> If you don't want to use pre-built binaries, specify branch `next` and `make
> release` as post action after plugin installation and update. e.g., `Plug
> 'autozimu/LanguageClient-neovim', {'branch': 'next', 'do': 'make release'}`.

> Android is not supported using the `install.sh` script.

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

# Install language servers

Install language servers if corresponding language servers are not available
yet on your system. Please see <http://langserver.org> and/or
<https://github.com/Microsoft/language-server-protocol/wiki/Protocol-Implementations>
for list of language servers.

# Configure this plugin

Example configuration

```vim
" Required for operations modifying multiple buffers like rename.
set hidden

let g:LanguageClient_serverCommands = {
    \ 'rust': ['~/.cargo/bin/rustup', 'run', 'stable', 'rls'],
    \ 'javascript': ['/usr/local/bin/javascript-typescript-stdio'],
    \ 'javascript.jsx': ['tcp://127.0.0.1:2089'],
    \ 'python': ['/usr/local/bin/pyls'],
    \ 'ruby': ['~/.rbenv/shims/solargraph', 'stdio'],
    \ }

nnoremap <silent> K :call LanguageClient#textDocument_hover()<CR>
nnoremap <silent> gd :call LanguageClient#textDocument_definition()<CR>
nnoremap <silent> <F2> :call LanguageClient#textDocument_rename()<CR>
```

# Troubleshooting

- Backup your vimrc and use [min-vimrc.vim](min-vimrc.vim) as vimrc.
- Try on [sample projects](tests/data).
- Execute `:echo &runtimepath` and make sure the plugin path is in the list.
- Make sure language server could be started when invoked manually from shell.
  Also try use absolute path for server commands, as PATH in vim might be
  different from shell env, especially on macOS.
- Check content of log file. Also worth noting language server might have
  separate log file.
