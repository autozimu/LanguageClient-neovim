FROM ubuntu:16.04

RUN apt-get update && \
    apt-get install --yes git curl

RUN apt-get install --yes software-properties-common && \
    add-apt-repository --yes ppa:neovim-ppa/stable && \
    apt-get update && \
    apt-get install --yes neovim python3-pip python3-pytest && \
    pip3 install neovim mypy flake8

RUN curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain=nightly-2017-11-20 -y

RUN ~/.cargo/bin/rustup component add rls-preview rust-analysis rust-src \
        --toolchain nightly-2017-11-20

# Verify rls works.
RUN timeout 5 ~/.cargo/bin/rustup run nightly-2017-11-20 rls

CMD /bin/bash
