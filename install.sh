#!/usr/bin/env bash
# Try install by
#   - download binary
#   - build with cargo

set -o nounset    # error when referencing undefined variable
set -o errexit    # exit when command fails

version=0.1.117
name=languageclient

die() {
    local _ret=$2
    test -n "$_ret" || _ret=1
    test "$_PRINT_HELP" = yes && print_help >&2
    echo "$1" >&2
    exit ${_ret}
}

begins_with_short_option() {
    local first_option all_short_options
    all_short_options='h'
    first_option="${1:0:1}"
    test "$all_short_options" = "${all_short_options/$first_option/}" && return 1 || return 0
}

print_help () {
    printf '%s\n' "The general script's help msg"
    printf 'Usage: %s [--(no-)force-build] [--(no-)force-fetch] [-h|--help]\n' "$0"
    printf '\t%s\n' "--force-build,--no-force-build: Forces local build (off by default)"
    printf '\t%s\n' "--force-fetch,--no-force-fetch: Forces binary fetch (off by default)"
    printf '\t%s\n' "-h,--help: Prints help"
}

force_build="off"
force_fetch="off"

# The parsing of the command-line
parse_args ()
{
    while test $# -gt 0
    do
        _key="$1"
        case "$_key" in
            --no-force-build|--force-build)
                force_build="on"
                test "${1:0:5}" = "--no-" && force_build="off"
                ;;
            --no-force-fetch|--force-fetch)
                force_fetch="on"
                test "${1:0:5}" = "--no-" && force_fetch="off"
                ;;
            -h|--help)
                print_help
                exit 0
                ;;
            -h*)
                print_help
                exit 0
                ;;
            *)
                _PRINT_HELP=yes die "FATAL ERROR: Got an unexpected argument '$1'" 1
                ;;
        esac
        shift
    done
    if [ ${force_build} == "on" ] && [ ${force_fetch} == "on" ]; then
        _PRINT_HELP=yes die "You cannot use --force-build and --force-fetch
        simultaneously." 1
    fi
}

try_curl() {
    command -v curl > /dev/null && \
        curl --fail --location "$1" --output bin/$name
}

try_wget() {
    command -v wget > /dev/null && \
        wget --output-document=bin/$name "$1"
}

download() {
    echo "Downloading bin/${name}..."
    url=https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/${1}
    if (try_curl "$url" || try_wget "$url"); then
        chmod a+x bin/$name
        return
    else
        try_build || echo "Prebuilt binary might not be ready yet. Please check minutes later."
    fi
}

try_build() {
    if command -v cargo > /dev/null; then
        echo "Trying build locally ..."
        make release
    else
        return 1
    fi
}

parse_args "$@"

rm -f bin/languageclient
if [ ${force_build} == "on" ]; then
    try_build
else
    arch=$(uname -sm)
    case "${arch}" in
        "Linux x86_64") download $name-$version-x86_64-unknown-linux-musl ;;
        "Linux i686") download $name-$version-i686-unknown-linux-musl ;;
        "Linux aarch64") download $name-$version-aarch64-unknown-linux-gnu ;;
        "Linux armv7"*) download $name-$version-armv7-unknown-musleabihf ;;
        "Darwin x86_64") download $name-$version-x86_64-apple-darwin ;;
        "FreeBSD amd64") download $name-$version-x86_64-unknown-freebsd ;;
        *)
            if [ ${force_fetch} == "off" ]; then
                echo "No pre-built binary available for ${arch}."; try_build
            fi
            ;;
    esac
fi
