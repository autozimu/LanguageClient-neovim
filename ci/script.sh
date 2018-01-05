#!/usr/bin/env bash

set -o nounset
set -o errexit
set -o xtrace

make test

if command -v docker > /dev/null ; then
    make integration-test-docker
fi
