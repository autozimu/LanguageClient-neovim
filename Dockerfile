FROM ubuntu:16.04

RUN apt-get update && \
    apt-get install --yes git

RUN apt-get install --yes software-properties-common && \
    add-apt-repository --yes ppa:neovim-ppa/stable && \
    apt-get update && \
    apt-get install --yes neovim && \
    apt-get install --yes python3-dev python3-pip && \
    pip3 install neovim mypy flake8

RUN apt-get install --yes python3-pytest

RUN apt-get install --yes curl

RUN curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain=nightly -y && \
    ~/.cargo/bin/rustup component add rls && \
    ~/.cargo/bin/rustup component add rust-analysis && \
    ~/.cargo/bin/rustup component add rust-src

RUN timeout 3 ~/.cargo/bin/rustup run nightly rls

RUN git clone --depth 1 https://github.com/junegunn/fzf.git /root/.fzf && /root/.fzf/install --bin

CMD /bin/bash
