FROM rust:1.46-slim

RUN apt-get update \
        && apt-get install --yes --no-install-recommends neovim curl git python3-pip python3-pytest mypy flake8 npm make \
        && apt-get clean \
        && rm -rf /var/lib/apt/lists/*

RUN python3 -m pip install neovim vim-vint

RUN rustup component add rustfmt clippy && rustup show
RUN curl -L https://github.com/rust-analyzer/rust-analyzer/releases/latest/download/rust-analyzer-linux -o /usr/local/bin/rust-analyzer \
        && chmod +x /usr/local/bin/rust-analyzer

ENV CARGO_TARGET_DIR=/tmp

CMD ["/bin/bash"]
