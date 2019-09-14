#!/bin/bash

set -o nounset
set -o errexit
set -o xtrace

function main() {
    make test

    if command -v docker > /dev/null ; then
        make integration-test-docker
    fi
}

# we don't run the "test phase" when doing deploys
if [ -z $TRAVIS_TAG ]; then
    main
fi
