#!/usr/bin/env bash

set -o xtrace

dir=$(dirname $(dirname $(realpath $0)))
cd $dir

LOG="${TMP:-/tmp}"/LanguageClient.log
LOG_SERVER="${TMP:-/tmp}"/LanguageServer.log

rm -f /tmp/nvim-LanguageClient-IntegrationTest
NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/data/vimrc --headless &
PID=$!
sleep 1s

py.test-3 --capture=no --exitfirst -v $@
ret=$?

if [[ $ret != 0 ]]; then
    cat $LOG
    cat $LOG_SERVER
fi

kill $PID
exit $ret
