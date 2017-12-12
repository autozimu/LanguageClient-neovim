#!/usr/bin/env bash

case $TRAVIS_OS_NAME in
    linux)
        cross build --target $TARGET --release
        cp --force target/$TARGET/release/$CRATE_NAME bin/
        ;;
    osx)
        make release
        ;;
esac

git add --force bin/$CRATE_NAME
git commit --message "Add binary for $TRAVIS_TAG-$TARGET."
tagname="binary-$TRAVIS_TAG-$TARGET"
git tag --force "$tagname"
git push --force origin "$tagname"
