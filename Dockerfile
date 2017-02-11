FROM archlinuxjp/archlinux:latest

RUN pacman -S --noconfirm base-devel \
    && paccache -r -k0

RUN pacman -S --noconfirm git cmake \
    && paccache -r -k0

RUN pacman -S --noconfirm neovim python-{neovim,pytest} \
    && paccache -r -k0

RUN pacman -S --noconfirm rustup \
    && paccache -r -k0 \
    && rustup default nightly

RUN git clone https://github.com/rust-lang-nursery/rls /opt/rls && cargo build --release --manifest-path=/opt/rls/Cargo.toml

CMD /bin/bash
