#!/usr/bin/env bash
# Try install by
#   - download binary
#   - build with cargo

set -o nounset    # error when referencing undefined variable
set -o errexit    # exit when command fails
set -o pipefail

version=0.1.0
name=languageclient

function try_curl() {
    command -v curl > /dev/null && \
        curl -fL $1 | tar -xzf - && \
        mv -f $name bin/
}

function try_wget() {
    command -v wget > /dev/null && \
        wget -O - $1 | tar -xzf - && \
        mv -f $name bin/
}

function download() {
    echo "Downloading bin/${name}..."
    local url=https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/${1}
    if (try_curl $url || try_wget $url); then
        return
    else
        echo "Failed to download with curl and wget"
        try_build
    fi
}

function try_build() {
    echo "Trying build locally ..."
    make release
}

arch=$(uname -sm)
binary=""
case "${arch}" in
    Linux\ *64) download $name-$version-x86_64-unknown-linux-musl.tar.gz ;;
    Linux\ *86) download $name-$version-i686-unknown-linux-musl.tar.gz ;;
    Darwin\ *64) download $name-$version-x86_64-apple-darwin.tar.gz ;;
    *) echo "No pre-built binary available for ${arch}."; try_build ;;
esac
