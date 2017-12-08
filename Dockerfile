FROM ubuntu:16.04

RUN apt-get update
RUN apt-get install --yes git curl
RUN curl -sL https://deb.nodesource.com/setup_8.x | bash -
RUN apt-get install --yes nodejs

RUN apt-get install --yes software-properties-common
RUN add-apt-repository --yes ppa:neovim-ppa/stable
RUN apt-get update
RUN apt-get install --yes neovim

RUN apt-get install --yes python3-pip python3-pytest
RUN pip3 install neovim mypy flake8

RUN npm install -g javascript-typescript-langserver

RUN curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain=nightly-2017-11-20 -y
RUN ~/.cargo/bin/rustup component add rls-preview rust-analysis rust-src \
        --toolchain nightly-2017-11-20
# Verify rls works.
RUN timeout 5 ~/.cargo/bin/rustup run nightly-2017-11-20 rls

CMD /bin/bash
