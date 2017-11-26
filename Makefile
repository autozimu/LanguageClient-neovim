all: build

build-docker-image: Dockerfile
	docker build --tag autozimu/languageclientneovim .

publish-docker-image:
	docker push autozimu/languageclientneovim

build:
	cargo fmt
	cargo build
	# cargo build --features clippy

release:
	cargo fmt
	cargo build --release
	mkdir -p bin
	cp --force target/release/languageclient bin/

test:
	cargo test

integration-test-install-dependencies:
	pip3 install neovim mypy flake8 --upgrade

integration-test-lint:
	mypy tests rplugin/python3/denite/source rplugin/python3/deoplete/sources
	flake8 .

integration-test: integration-test-lint
	tests/test.sh
