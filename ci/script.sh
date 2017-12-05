# This script takes care of testing your crate

set -o errtrace
set -o xtrace

make test

if [[ ${INTEGRATION_TEST:-0} == 1 ]]; then
    docker run --volume $(pwd):/root/.config/nvim autozimu/languageclientneovim bash -c "\
        export PATH=$PATH:~/.cargo/bin && \
        export CARGO_TARGET_DIR=/tmp && \
        cd /root/.config/nvim && \
        make integration-test"
fi
