# This script takes care of testing your crate

set -o errtrace
set -o xtrace

make test

if command -v docker > /dev/null ; then
    make integration-test-docker
fi
