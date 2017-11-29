#!/usr/bin/env sh

# set -o nounset    # error when referencing undefined variable
set -o errexit    # exit when command fails

LOGFILE=/tmp/LanguageClient.log

cat /dev/null > $LOGFILE
exec 2>$LOGFILE

DIR=$(dirname $(realpath $0))

if [[ -n "$LANGUAGECLIENT_DEBUG" ]]; then
    ~/.cargo/bin/cargo build --manifest-path=$DIR/Cargo.toml
    export RUST_LOG=languageclient=info
    exec $DIR/target/debug/languageclient
else
    export RUST_LOG=languageclient=warn
    exec $DIR/bin/languageclient
fi
