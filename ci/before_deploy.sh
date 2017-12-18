# This script takes care of building your crate and packaging it for release

set -ex

package() {
    local TARGET=$1 \
        src=$(pwd) \
        stage=

    case $TRAVIS_OS_NAME in
        linux)
            stage=$(mktemp -d)
            ;;
        osx)
            stage=$(mktemp -d -t tmp)
            ;;
    esac

    test -f Cargo.lock || cargo generate-lockfile

    cross build --target $TARGET --release
    cp target/$TARGET/release/$BIN_NAME $stage/

    cd $stage
    tar czf $src/$CRATE_NAME-$TRAVIS_TAG-$TARGET.tar.gz *
    cd $src

    rm -rf $stage
}

release_tag() {
    case $TRAVIS_OS_NAME in
        linux)
            cross build --target $TARGET --release
            cp --force target/$TARGET/release/$BIN_NAME bin/
            ;;
        osx)
            make release
            ;;
    esac

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

TARGETS=(${TARGETS//:/ })
for TARGET in "${TARGETS[@]}"; do
    if [[ $TARGET =~ .*windows.* ]]; then
        BIN_NAME=$CRATE_NAME.exe
    else
        BIN_NAME=$CRATE_NAME
    fi

    release_tag $TARGET
    package $TARGET
done
