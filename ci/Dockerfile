FROM rust:1.27-slim

RUN apt-get update \
        && apt-get install --yes curl git neovim python3-pip python3-pytest mypy flake8 \
        && pip3 install neovim vim-vint

RUN curl -sL https://deb.nodesource.com/setup_8.x | bash - \
        && apt-get install --yes nodejs \
        && npm install -g javascript-typescript-langserver@2.5.5

RUN rustup component add rls-preview rust-analysis rust-src \
        && rustc --version \
        && rls --version

ENV CARGO_TARGET_DIR=/tmp

CMD /bin/bash
