all: release

dev: build fmt clippy vim-lint

ci: vim-lint python-lint check-fmt-and-clippy test integration-test

check-fmt-and-clippy:
	cargo check
	cargo fmt -- --check
	cargo clippy -- --deny warnings

build:
	cargo build

fmt:
	cargo fmt

clippy:
	cargo clippy

vim-lint:
	vint autoload plugin

release:
	cargo build --release
	[ -z "${CARGO_TARGET_DIR}" ] && \
		cp -f target/release/languageclient bin/ || \
		cp -f ${CARGO_TARGET_DIR}/release/languageclient bin/
	chmod a+x bin/languageclient

bump-version:
	[[ `git rev-parse --abbrev-ref HEAD` == "dev" ]] || (echo "Not on branch dev"; exit 1)
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
		curl -O -L https://golang.org/dl/go1.16.2.linux-amd64.tar.gz && \
		rm -rf /usr/local/go && tar -C /usr/local -xzf go1.16.2.linux-amd64.tar.gz && \
		export PATH=$$PATH:/usr/local/go/bin:/usr/local/cargo/bin && \
		export GOPATH=/usr/local/go && \
		go install golang.org/x/tools/gopls@v0.6.6 && \
		export CARGO_TARGET_DIR=/tmp && \
		cd /root/.config/nvim && \
		make integration-test"

integration-test-docker-debug:
	docker run --interactive --tty --volume ${CURDIR}:/root/.config/nvim autozimu/languageclientneovim

clean:
	cargo clean
	rm -rf bin/languageclient

DATE := $(shell date -u +%F)

build-docker-image: ci/Dockerfile
	docker build \
		--tag autozimu/languageclientneovim:latest \
		--tag autozimu/languageclientneovim:$(DATE) \
		ci

publish-docker-image:
	docker push autozimu/languageclientneovim:latest
	docker push autozimu/languageclientneovim:$(DATE)
