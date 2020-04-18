# Prerequisites

- [rust] toolchain

[rust]: https://www.rust-lang.org

# Development

Put those settings inside vimrc,

```vim
let g:LanguageClient_devel = 1 " Use rust debug build
let g:LanguageClient_loggingLevel = 'DEBUG' " Use highest logging level
```

- Make necessary changes.
- Build. `make` to build, format and run [clippy], or `make build` to run build only.
- Verify changes.
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
