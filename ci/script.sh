# This script takes care of testing your crate

set -ex

main() {
    make test

    if [[ ${INTEGRATION_TEST:-0} == 1 ]]; then
        make integration-test
    fi

    cross build --target $TARGET --release
}

# we don't run the "test phase" when doing deploys
if [ -z $TRAVIS_TAG ]; then
    main
fi
