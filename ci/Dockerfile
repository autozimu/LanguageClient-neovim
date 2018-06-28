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

# The first mkdir is a workaround for the following debian bug:
# https://bugs.debian.org/cgi-bin/bugreport.cgi?bug=863199
RUN mkdir -p /usr/share/man/man1 \
        && apt-get install --yes openjdk-8-jdk-headless \
        && mkdir -p /jdtls \
        && cd /jdtls \
        && curl -sL http://download.eclipse.org/jdtls/snapshots/jdt-language-server-latest.tar.gz | tar -xz \
        && cd plugins \
        && ln -s org.eclipse.equinox.launcher_*.jar org.eclipse.equinox.launcher_.jar

ENV CARGO_TARGET_DIR=/tmp

CMD /bin/bash
