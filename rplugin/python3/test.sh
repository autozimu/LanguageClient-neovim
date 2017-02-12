#!/usr/bin/bash

set -o verbose

rm -f /tmp/nvim-LanguageClient-IntegrationTest
cat /dev/null > /tmp/LanguageClient.log
rm -rf LanguageClient/__pycache__

nvim +UpdateRemotePlugins +qall
NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/vimrc --headless 2>/dev/null &
PID=$!
sleep 1s

py.test --capture=no --exitfirst
ret=$?
cat /tmp/LanguageClient.log

kill $PID
exit $ret
