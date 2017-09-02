#!/usr/bin/env bash

set -o verbose

if [[ -z "$TMP" ]]; then
    TMP=/tmp
fi
LOG="$TMP"/LanguageClient.log

rm -f /tmp/nvim-LanguageClient-IntegrationTest
cat /dev/null > $LOG
rm -rf LanguageClient/__pycache__

nvim +UpdateRemotePlugins +qall
NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/vimrc --headless 2>/dev/null &
PID=$!
sleep 1s

py.test-3 --capture=no --exitfirst -v $@
ret=$?
cat $LOG

kill $PID
exit $ret
