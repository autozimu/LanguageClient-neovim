#!/usr/bin/env sh

set -o nounset    # error when referencing undefined variable
set -o errexit    # exit when command fails

LOGFILE=/tmp/LanguageClient.log
LOGFILE_SERVER=/tmp/LanguageServer.log
cat /dev/null > $LOGFILE
cat /dev/null > $LOGFILE_SERVER

DIR=$(dirname $(realpath $0))

exec 2>$LOGFILE

cargo --version >&2
cargo build --manifest-path=$DIR/Cargo.toml
$DIR/target/debug/languageclient --version >&2
export RUST_LOG=languageclient=debug
$DIR/target/debug/languageclient
