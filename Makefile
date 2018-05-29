all: check build fmt clippy

check:
	cargo check

build:
	cargo build

fmt:
	cargo fmt

clippy:
	cargo +nightly clippy

vint:
	vint .

release:
	cargo build --release
	cp -f target/release/languageclient bin/
	chmod a+x bin/languageclient

bump-version:
	cargo release patch

test:
	cargo test

integration-test-lint:
	mypy --ignore-missing-imports \
		tests \
		rplugin/python3/denite/source \
		rplugin/python3/deoplete/sources
	flake8 .

integration-test: build integration-test-lint
	tests/test.sh

integration-test-docker:
	docker image pull autozimu/languageclientneovim
	docker run --volume ${CURDIR}:/root/.config/nvim autozimu/languageclientneovim bash -c "\
		export CARGO_TARGET_DIR=/tmp && \
		cd /root/.config/nvim && \
		make integration-test"

integration-test-docker-debug:
	docker image pull autozimu/languageclientneovim
	docker run --interactive --tty --volume ${CURDIR}:/root/.config/nvim autozimu/languageclientneovim

cleanup-binary-tags:
	ci/cleanup-binary-tags.py

clean:
	cargo clean
	rm -rf bin/languageclient

build-docker-image: ci/Dockerfile
	docker build --tag autozimu/languageclientneovim ci

publish-docker-image:
	docker push autozimu/languageclientneovim
