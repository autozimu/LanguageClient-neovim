all: fmt build

fmt:
	cargo +nightly fmt

clippy:
	cargo +nightly clippy

build:
	cargo build

release:
	cargo build --release
	cp --force target/release/languageclient bin/

test:
	cargo test

integration-test-install-dependencies:
	sudo add-apt-repository --yes ppa:neovim-ppa/stable
	sudo apt-get update
	sudo apt-get install --yes neovim python3-pip python3-pytest
	pip3 install neovim mypy flake8
	rustup component add rls-preview rust-analysis rust-src

integration-test-lint:
	mypy tests rplugin/python3/denite/source rplugin/python3/deoplete/sources
	flake8 .

integration-test: build integration-test-lint
	tests/test.sh
