#!/usr/bin/env bash

set -o xtrace

dir=$(dirname $(dirname $(realpath $0)))
cd $dir

LOG=~/.local/share/nvim/LanguageClient.log

curl -fLo tests/data/.vim/autoload/plug.vim --create-dirs \
    https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim

nvim -n -u tests/data/vimrc --headless +PlugInstall +qa
rm -f /tmp/nvim-LanguageClient-IntegrationTest
if [[ "$TMUX" ]]; then
    tmux split-window 'NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/data/vimrc'
else
    NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/data/vimrc --headless 2>/dev/null &
fi
PID=$!
sleep 1s

$(command -v pytest-3 || echo pytest) --capture=no --exitfirst -v $@
ret=$?

if [[ $ret != 0 ]]; then
    cat $LOG
fi

if [[ -n $PID ]]; then
    kill $PID
fi
exit $ret
