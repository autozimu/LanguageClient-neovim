#!/usr/bin/env bash

set -o verbose

LOG="${TMP:-/tmp}"/LanguageClient.log
LOG_SERVER="${TMP:-/tmp}"/LanguageServer.log

rm -f /tmp/nvim-LanguageClient-IntegrationTest
cat /dev/null > $LOG
cat /dev/null > $LOG_SERVER

NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/data/vimrc --headless 2>/dev/null &
PID=$!
sleep 1s

py.test-3 --capture=no --exitfirst -v $@
ret=$?
cat $LOG
cat $LOG_SERVER

kill $PID
exit $ret
