> For legacy python implementation, see [branch master](https://github.com/autozimu/LanguageClient-neovim/tree/master).

# LanguageClient-neovim

[![CircleCI](https://circleci.com/gh/autozimu/LanguageClient-neovim.svg?style=svg)](https://circleci.com/gh/autozimu/LanguageClient-neovim) [![Join the chat at https://gitter.im/LanguageClient-neovim/LanguageClient-neovim](https://badges.gitter.im/LanguageClient-neovim/LanguageClient-neovim.svg)](https://gitter.im/LanguageClient-neovim/LanguageClient-neovim?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

[Language Server Protocol](LSP) support for [vim] and [neovim].

[language server protocol]: https://github.com/Microsoft/language-server-protocol
[neovim]: https://neovim.io/
[vim]: http://www.vim.org/

![rename](https://cloud.githubusercontent.com/assets/1453551/24251636/2e73a1cc-0fb1-11e7-8a5e-3332e6a5f424.gif)

More recordings at [Updates, screenshots & GIFs](https://github.com/autozimu/LanguageClient-neovim/issues/35).

# Features

- Non-blocking asynchronous calls.
- [Sensible completion](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731936).
  Integrated well with [deoplete](https://github.com/Shougo/deoplete.nvim) or
  [ncm2](https://github.com/ncm2/ncm2), or [MUcomplete](https://github.com/lifepillar/vim-mucomplete).
  Or simply with vim built-in `omnifunc`.
- [Realtime diagnostics/compiler/lint message.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288732042)
- [Rename.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731403)
- [Hover/Get identifier info.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731665)
- [Goto definition.](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731744)
- Goto reference locations.
- [Workspace/Document symbols query](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-288731839).
- [Formatting](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-324497559).
- [Code Action/Quick Fix](https://github.com/autozimu/LanguageClient-neovim/issues/35#issuecomment-331016526).

# Quick Start

Using [`vim-plug`](https://github.com/junegunn/vim-plug):

```vim
Plug 'autozimu/LanguageClient-neovim', {
    \ 'branch': 'next',
    \ 'do': 'bash install.sh',
    \ }

" (Optional) Multi-entry selection UI.
Plug 'junegunn/fzf'
```

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

" note that if you are using Plug mapping you should not use `noremap` mappings.
nmap <F5> <Plug>(lcn-menu)
" Or map each action separately
nmap <silent>K <Plug>(lcn-hover)
nmap <silent> gd <Plug>(lcn-definition)
nmap <silent> <F2> <Plug>(lcn-rename)
```

Run command `nvim +PlugInstall +UpdateRemotePlugins +qa` in shell to install
this plugin. Install corresponding language servers. Restart neovim/vim and
language services will be available right away. Happy hacking!

# Mappings

LanguageClient-neovim defines various Plug mappings, see `:help LanguageClientMappings` for a full
list and an example configuration.

# Install

[Full installation steps](INSTALL.md)

# Language Servers

**Note**, you will also need language server(s) to take advantages of
this plugin. To find list of language server implementations and how
to install them, please see <http://langserver.org> and/or
<https://microsoft.github.io/language-server-protocol/implementors/servers/>.

# Documentation

- [`:help LanguageClient`](doc/LanguageClient.txt)
- [Changelog](CHANGELOG.md)
- [Troubleshooting](INSTALL.md#troubleshooting)
- [Contributing](.github/CONTRIBUTING.md)
- [The MIT License](LICENSE.txt)
