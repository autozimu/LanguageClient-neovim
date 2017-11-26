# This script takes care of testing your crate

set -ex

main() {
    cargo test
    # make integration-test
}

# we don't run the "test phase" when doing deploys
if [ -z $TRAVIS_TAG ]; then
    main
fi
