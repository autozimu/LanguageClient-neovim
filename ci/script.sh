# This script takes care of testing your crate

set -o errtrace
set -o xtrace

make test

if [[ ${INTEGRATION_TEST:-0} == 1 ]]; then
    make integration-test-docker
fi
