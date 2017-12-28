all: build fmt clippy

build:
	cargo check
	cargo build

fmt:
	cargo +nightly fmt

clippy:
	cargo +nightly clippy

release:
	cargo build --release
	cp -f target/release/languageclient bin/

bump-version:
	cargo release --level=patch

test:
	cargo test

integration-test-lint:
	mypy tests rplugin/python3/denite/source rplugin/python3/deoplete/sources
	flake8 .

integration-test: build integration-test-lint
	tests/test.sh

integration-test-docker:
	docker run --volume ${CURDIR}:/root/.config/nvim autozimu/languageclientneovim bash -c "\
		export PATH=$$PATH:~/.cargo/bin && \
		export CARGO_TARGET_DIR=/tmp && \
		cd /root/.config/nvim && \
		make integration-test"

cleanup-binary-tags:
	ci/cleanup-binary-tags.py

clean:
	cargo clean
	rm -rf bin/languageclient

build-docker-image: Dockerfile
	docker build --tag autozimu/languageclientneovim .

publish-docker-image:
	docker push autozimu/languageclientneovim
