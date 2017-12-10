# 1. Install neovim or vim

Obviously you need [neovim](https://github.com/neovim/neovim#install-from-package) or [vim](http://www.vim.org/)!

# 2. Install dependencies

For Windows user, [Microsoft Visual C++ 2015 runtime] is needed.

[Microsoft Visual C++ 2015 runtime]: https://www.microsoft.com/en-us/download/details.aspx?id=52685

# 3. Install this plugin
Choose steps matching your plugin manager.

> For Windows user, replace all following `install.sh` with `install.ps1`.

## [vim-plug](https://github.com/junegunn/vim-plug) user
Add following to vimrc
```vim
Plug 'autozimu/LanguageClient-neovim', {'branch': 'next',  'do': './install.sh' }
```

Restart neovim and run `:PlugInstall` to install this plugin.

## [dein.vim](https://github.com/Shougo/dein.vim) user
For MacOS and Linux user, add following to vimrc
```vim
call dein#add('autozimu/LanguageClient-neovim', {
    \ 'rev': 'next',
    \ 'build': './install.sh',
    \ })
```

Restart neovim and run `:call dein#install()` to install this plugin.

## Manual
Clone this repo into some place, e.g., '~/.vim-plugins'
```sh
mkdir -p ~/.vim-plugins
cd ~/.vim-plugins
git clone -b next --single-branch https://github.com/autozimu/LanguageClient-neovim.git
cd LanguageClient-neovim
./install.sh
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
    \ 'rust': ['rustup', 'run', 'nightly', 'rls'],
    \ 'javascript': ['javascript-typescript-stdio'],
    \ }

nnoremap <silent> K :call LanguageClient_textDocument_hover()<CR>
nnoremap <silent> gd :call LanguageClient_textDocument_definition()<CR>
nnoremap <silent> <F2> :call LanguageClient_textDocument_rename()<CR>
```

# 6. Troubleshooting

1. Begin with something small.
    - Backup your vimrc and use [min-init.vim](https://github.com/autozimu/LanguageClient-neovim/blob/next/min-init.vim) as vimrc.
    - Try with [sample projects](https://github.com/autozimu/LanguageClient-neovim/tree/next/tests/data).
1. Run `:echo &runtimepath` and make sure the plugin path is in the list.
1. Make sure language server could be started when invoked manually from shell.
