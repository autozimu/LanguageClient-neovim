FROM rust

RUN curl -sL https://deb.nodesource.com/setup_8.x | bash -
RUN apt-get install --yes nodejs

RUN apt-get install --yes neovim

RUN apt-get install --yes python3-pip python3-pytest mypy flake8
RUN pip3 install neovim vim-vint

RUN npm install -g javascript-typescript-langserver

RUN rustup component add rls-preview rust-analysis rust-src
RUN rls --version

ENV CARGO_TARGET_DIR=/tmp

CMD /bin/bash
