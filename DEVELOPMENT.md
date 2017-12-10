# Prerequisites
- [rust] toolchain

[rust]: https://www.rust-lang.org

# Development
- Make necessary changes.
- Build. `make` to build, format and run [clippy], or `make build` to run build only.
- Verify changes.
- Run tests

[clippy]: https://github.com/rust-lang-nursery/rust-clippy

# Run tests
(Option 1) Refer [`Dockerfile`](Dockerfile) to install integration tests dependencies.
```sh
make test && make integration-test
```

(Option 2) With docker installed,
```sh
make test && make integration-test-docker
```
