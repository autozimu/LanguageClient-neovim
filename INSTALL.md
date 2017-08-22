# 1. Install neovim

Obviously you need [neovim](https://github.com/neovim/neovim#install-from-package)!

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

# Troubleshooting

- Begin with something small.
    - Backup your init.vim and use [min-init.vim](https://github.com/autozimu/LanguageClient-neovim/blob/master/min-init.vim) as init.vim, run `nvim +PlugInstall +UpdateRemotePlugins +qa` command in shell.
    - Try with [sample projects](https://github.com/autozimu/LanguageClient-neovim/tree/master/rplugin/python3/tests).
- Run `:CheckHealth` to see if there is issue with neovim python3 host.
  then start neovim normally.
- Run `:echo &runtimepath` and make sure the plugin path is in the list.
- Make sure your language server run properly when invoked manually from
  shell.
