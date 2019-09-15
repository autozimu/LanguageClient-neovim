#!/bin/bash

set -o nounset
set -o errexit
set -o xtrace

package() {
    BIN_NAME_TAG=$CRATE_NAME-$TRAVIS_TAG-$TARGET
    if [[ $TARGET =~ .*windows.* ]]; then
        BIN_NAME_TAG=$BIN_NAME_TAG.exe
    fi

    cp -f target/$TARGET/release/$BIN_NAME bin/$BIN_NAME_TAG
    sha256sum "bin/$BIN_NAME_TAG" | tee "bin/$BIN_NAME_TAG.sha256"
}

release_tag() {
    cp -f target/$TARGET/release/$BIN_NAME bin/

    git config --global user.email "travis@travis-ci.org"
    git config --global user.name "Travis CI"

    git add --force bin/$BIN_NAME
    SHA=$(git rev-parse --short HEAD)
    git commit --message "Add binary. $SHA. $TRAVIS_TAG-$TARGET."
    tagname="binary-$TRAVIS_TAG-$TARGET"
    git tag --force "$tagname"
    git push --force https://${GITHUB_TOKEN}@github.com/autozimu/LanguageClient-neovim.git "$tagname"

    git reset --hard HEAD^
}

if [[ $TRAVIS_OS_NAME == 'osx' ]]; then
    export PATH="/usr/local/opt/coreutils/libexec/gnubin:$PATH"
fi

TARGETS=(${TARGETS//:/ })
for TARGET in "${TARGETS[@]}"; do
    BIN_NAME=$CRATE_NAME
    if [[ $TARGET =~ .*windows.* ]]; then
        BIN_NAME=$BIN_NAME.exe
    fi

    cross build --release --target $TARGET
    release_tag
    package
done

if [[ $TRAVIS_OS_NAME == 'linux' ]]; then
    sudo apt-get update
    sudo apt-get install --yes python3-pip
    sudo pip3 install semver

    ci/cleanup-binary-tags.py
fi
