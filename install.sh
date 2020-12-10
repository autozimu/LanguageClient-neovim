#!/usr/bin/env sh
# Try install by
#   - download binary
#   - build with cargo

set -o nounset    # error when referencing undefined variable
set -o errexit    # exit when command fails

version=0.1.161
name=languageclient

arch=$(uname -sm)

try_curl() {
    command -v curl > /dev/null && \
        curl --fail --location "$1" --output bin/$name
}

try_wget() {
    command -v wget > /dev/null && \
        wget --output-document=bin/$name "$1"
}

download() {
    echo "Trying download bin/${name} ${version}..."
    url=https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/${1}
    if (try_curl "$url" || try_wget "$url"); then
        chmod a+x bin/$name
        return
    else
        echo "Prebuilt binary is not available for:" "${arch}"
        try_build
    fi
}

try_build() {
    if command -v cargo > /dev/null; then
        echo "Trying build locally ${version} ..."
        make release
    else
        echo "cargo is not available. Abort."
        return 1
    fi
}

bin=bin/languageclient
if [ -f "$bin" ]; then
    installed_version=$($bin -V)
    case "${installed_version}" in
        *${version}*) echo "Version is equal to ${version}, skip install." ; exit 0 ;;
        *) rm -f "$bin" ;;
    esac
fi

case "${arch}" in
    "Linux x86_64") download $name-$version-x86_64-unknown-linux-musl ;;
    "Linux i686") download $name-$version-i686-unknown-linux-musl ;;
    "Linux aarch64") download $name-$version-aarch64-unknown-linux-musl ;;
    "Darwin x86_64") download $name-$version-x86_64-apple-darwin ;;
    "FreeBSD amd64") download $name-$version-x86_64-unknown-freebsd ;;
    *) echo "No pre-built binary available for ${arch}."; try_build ;;
esac
