# Prerequisites

- [rust] toolchain or [nix]

[rust]: https://www.rust-lang.org
[nix]: https://nixos.org/

# Development

Clone this repo into some place, e.g., `~/.vim-plugins`

```sh
mkdir -p ~/.vim-plugins
cd ~/.vim-plugins
git clone https://github.com/autozimu/LanguageClient-neovim.git
cd LanguageClient-neovim
git checkout dev
# nix users can execute nix-shell at this point
make dev
```

Add this plugin to vim/neovim `runtimepath`,

```vim
set runtimepath+=~/.vim-plugins/LanguageClient-neovim
```

Put those settings inside vimrc,

```vim
let g:LanguageClient_devel = 1 " Use rust debug build
let g:LanguageClient_loggingLevel = 'INFO' " Optional, use higher logging level
```

- Make necessary changes.
- Execute `make dev` to build, format and [lint][clippy].
- Running separate vim/neovim. Verify changes.
- Run tests. (See below section)

[clippy]: https://github.com/rust-lang-nursery/rust-clippy

# Run tests

(Option 1. Recommended) With docker installed,

```sh
make test && make integration-test-docker
```

(Option 2) Refer [`Dockerfile`](Dockerfile) to install tests dependencies.

```sh
make test && make integration-test
```

# Submit PR

Please submit pull request to `dev` branch. This is to avoid mismatch between
vimscript and rust binary run by end user.

# Release
1. Update [CHANGELOG](../CHANGELOG.md).
1. Issue command `cargo release patch`. Note you will need [`cargo-release`][cargo-release] installed. This will create a commit with updated version metadata, tag it, push to GitHub remote, which will then trigger Travis workflow to generate binaries.
1. Once Travis workflow is finished successfully, rebase `dev` branch onto `next` branch.

[cargo-release]: https://github.com/sunng87/cargo-release
