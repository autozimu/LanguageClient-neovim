#!/usr/bin/env bash

set -o nounset    # error when referencing undefined variable
set -o errexit    # exit when command fails

LOGFILE=/tmp/LanguageClient.log

cat /dev/null > $LOGFILE
exec 2>$LOGFILE

DIR=$(dirname $(realpath $0))

cargo build --manifest-path=$DIR/Cargo.toml
export RUST_LOG=languageclient=info
$DIR/target/debug/languageclient
