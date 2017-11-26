# This script takes care of testing your crate

set -ex

make test

if [[ ${INTEGRATION_TEST:-0} == 1 ]]; then
    make integration-test-install-dependencies
    make integration-test
fi
