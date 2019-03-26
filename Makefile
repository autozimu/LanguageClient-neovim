all: release

dev: build fmt clippy vint

ci: vint python-lint test integration-test
	cargo fmt -- --check

check:
	cargo check

build:
	cargo build

fmt:
	cargo fmt

clippy:
	cargo clippy

vint:
	vint autoload plugin

release:
	RUSTFLAGS="-C target-cpu=native" cargo build --release
	cp -f target/release/languageclient bin/
	chmod a+x bin/languageclient

bump-version:
	[[ `git rev-parse --abbrev-ref HEAD` == "next" ]] || (echo "Not on branch next"; exit 1)
	cargo release patch

test:
	cargo test

python-lint:
	mypy --ignore-missing-imports \
		tests \
		rplugin/python3/denite/source \
		rplugin/python3/deoplete/sources
	flake8 .

integration-test: build
	tests/test.sh

integration-test-docker:
	docker run --volume ${CURDIR}:/root/.config/nvim autozimu/languageclientneovim bash -c "\
		export CARGO_TARGET_DIR=/tmp && \
		cd /root/.config/nvim && \
		make integration-test"

integration-test-docker-debug:
	docker run --interactive --tty --volume ${CURDIR}:/root/.config/nvim autozimu/languageclientneovim

cleanup-binary-tags:
	ci/cleanup-binary-tags.py

clean:
	cargo clean
	rm -rf bin/languageclient

DATE := $(shell date +%F)

build-docker-image: ci/Dockerfile
	docker build \
		--tag autozimu/languageclientneovim:latest \
		--tag autozimu/languageclientneovim:$(DATE) \
		ci

publish-docker-image:
	docker push autozimu/languageclientneovim:latest
	docker push autozimu/languageclientneovim:$(DATE)
