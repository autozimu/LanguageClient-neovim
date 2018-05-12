FROM rust:1.26

RUN curl -sL https://deb.nodesource.com/setup_8.x | bash - \
        && apt-get install --yes nodejs \
        && npm install -g javascript-typescript-langserver@2.5.5

RUN apt-get install --yes neovim python3-pip python3-pytest mypy flake8 \
        && pip3 install neovim vim-vint

RUN rustup component add rls-preview rust-analysis rust-src \
        && rls --version

ENV CARGO_TARGET_DIR=/tmp

CMD /bin/bash
