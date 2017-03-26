# neovim

Obviously you need [neovim](https://github.com/neovim/neovim#install-from-package)!

# python-neovim

Run following command to install neovim python plugin host:
```
sudo pip3 install --upgrade neovim
```

# LanguageClient-neovim

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

## [dein.vim](https://github.com/Shougo/dein.vim) user
```
call dein#add('autozimu/LanguageClient-neovim')
```
Restart neovim and run `:call dein#install()`.

## Manual
Clone this repo into some place and add the folder into neovim runtimepath.

# Register this plugin

Run `:UpdateRemotePlugins` in neovim and restart.

At this point, your `~/.local/share/nvim/rplugin.vim` should contains
information about this plugin. If not, see following trouble shooting.

# Trouble Shooting

- Run `:CheckHealth` to see if there is issue with neovim python3 host.
- Try backup your init.vim and use
  [min-init.vim](https://github.com/autozimu/LanguageClient-neovim/blob/master/min-init.vim)
  as your init.vim, run `nvim +PlugInstall +UpdateRemotePlugins +qa` in shell,
  then start neovim normally.
- Run `:echo &runtimepath` and make sure the plugin path is in the list.
- Make sure your language server run properly when invoked manually from
  shell.
