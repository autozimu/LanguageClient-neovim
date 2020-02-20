FROM rust:1.41-slim

RUN apt-get update \
        && apt-get install --yes --no-install-recommends neovim curl git python3-pip python3-pytest mypy flake8 npm make \
        && apt-get clean \
        && rm -rf /var/lib/apt/lists/*

RUN python3 -m pip install neovim vim-vint

RUN npm install -g javascript-typescript-langserver@2.5.5

RUN rustup component add rustfmt rls rust-analysis rust-src \
        && rustc --version \
        && rls --version

ENV CARGO_TARGET_DIR=/tmp

CMD ["/bin/bash"]
