FROM rust:1.33-slim

RUN echo 'deb http://deb.debian.org/debian stretch-backports main contrib non-free' >> /etc/apt/sources.list

RUN apt-get update \
        && apt-get install --yes --target-release stretch-backports neovim \
        && apt-get install --yes curl git python3-pip python3-pytest mypy flake8 \
        && pip3 install neovim vim-vint

RUN curl -sL https://deb.nodesource.com/setup_8.x | bash - \
        && apt-get install --yes nodejs \
        && npm install -g javascript-typescript-langserver@2.5.5

RUN rustup component add rustfmt rls rust-analysis rust-src \
        && rustc --version \
        && rls --version

ENV CARGO_TARGET_DIR=/tmp

CMD /bin/bash
