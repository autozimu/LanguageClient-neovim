FROM ubuntu:16.04

RUN apt-get update && \
    apt-get install --yes git curl

RUN apt-get install --yes software-properties-common && \
    add-apt-repository --yes ppa:neovim-ppa/stable && \
    apt-get update && \
    apt-get install --yes neovim python3-pip python3-pytest && \
    pip3 install neovim mypy flake8

RUN curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain=nightly-2017-11-20 -y

RUN ~/.cargo/bin/rustup component add rls-preview rust-analysis rust-src

# Verify rls works.
RUN timeout 3 ~/.cargo/bin/rls

RUN ~/.cargo/bin/cargo install rustfmt-nightly

RUN git clone --depth 1 https://github.com/junegunn/fzf.git /root/.fzf && /root/.fzf/install --bin

CMD /bin/bash
