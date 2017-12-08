# 1. Install neovim or vim

Obviously you need [neovim](https://github.com/neovim/neovim#install-from-package) or [vim](http://www.vim.org/)!

# 2. Install python modules
## python-neovim
Run following command to install neovim python plugin host:
```
sudo pip3 install --upgrade neovim
```

## typing
_Note: Already included in Python version 3.6.0 and later_.

Run following command to install `typing` module:
```
sudo pip3 install --upgrade typing
```

# 3.0 (Vim only)

For vim, two other plugins are necessary to make this one work (I'm sorry,
but vim support is a second thought.)

- [vim-hug-neovim-rpc](https://github.com/roxma/vim-hug-neovim-rpc)
- [nvim-yarp](https://github.com/roxma/nvim-yarp)

Also, following `:UpdateRemotePlugins` commands is not needed for vim users.

# 3. Install this plugin
Choose steps matching your plugin manager.

## [vim-plug](https://github.com/junegunn/vim-plug) user
Add following to vimrc
```
Plug 'autozimu/LanguageClient-neovim', { 'do': ':UpdateRemotePlugins' }
```
Restart neovim and run `:PlugInstall`.

## [Vundle](https://github.com/VundleVim/Vundle.vim) user
Add following to vimrc
```
Plugin 'autozimu/LanguageClient-neovim'

```
Restart neovim and run `:PluginInstall`.

When using Vundle you need to run `:UpdateRemotePlugins` command manually 
after you install/update the plugin. 

## [dein.vim](https://github.com/Shougo/dein.vim) user
```
call dein#add('autozimu/LanguageClient-neovim')
```
Restart neovim and run `:call dein#install()`.

## Manual
Clone this repo into some place and add the folder into neovim runtimepath.

# 4. Register this plugin

Run `:UpdateRemotePlugins` in neovim and restart.

At this point, your `~/.local/share/nvim/rplugin.vim` should contains
information about this plugin. If not, see following troubleshooting.

# 5. Install language servers
Install language servers if corresponding language servers are not available
yet on your system. Please see <http://langserver.org> and/or
<https://github.com/Microsoft/language-server-protocol/wiki/Protocol-Implementations>
for list of language servers.

# 6. Configure this plugin
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

# 7. Troubleshooting

1. Begin with something small.
    - Backup your init.vim and use [min-init.vim](https://github.com/autozimu/LanguageClient-neovim/blob/master/min-init.vim) as init.vim, run `nvim +PlugInstall +UpdateRemotePlugins +qa` command in shell.
    - Try with [sample projects](https://github.com/autozimu/LanguageClient-neovim/tree/master/rplugin/python3/tests).
1. Run `:CheckHealth` to see if there is issue with neovim python3 host.  then
   start neovim normally.
1. Run `:echo &runtimepath` and make sure the plugin path is in the list.
1. Make sure language server run properly when invoked manually from shell.

# 8. Known issues

Q: Single 'd' deletes a line <https://github.com/autozimu/LanguageClient-neovim/issues/132>

A: This is a bug relates to timer in neovim version <= 0.2. Please upgrade to
neovim 0.2.1 or above.
