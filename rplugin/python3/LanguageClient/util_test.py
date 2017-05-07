from . util import (
    joinPath, getRootPath, pathToURI, uriToPath, escape,
    getGotoFileCommand)


def test_getRootPath():
    assert (getRootPath(joinPath("tests/sample-rs/src/main.rs"), "rust") ==
            joinPath("tests/sample-rs"))


def test_pathToURI():
    assert (pathToURI("/tmp/sample-rs/src/main.rs") ==
            "file:///tmp/sample-rs/src/main.rs")


def test_pathToURIRelative():
    assert pathToURI(".") is None


def testUriToPath():
    assert (uriToPath("file:///tmp/sample-rs/src/main.rs") ==
            "/tmp/sample-rs/src/main.rs")


def test_escape():
    assert escape("my' precious") == "my'' precious"


def test_getGotoFileCommand():
    assert getGotoFileCommand("/tmp/sample", [
        "/tmp/sample",
        "/tmp/somethingelse"
    ]) == "buffer /tmp/sample"

    assert getGotoFileCommand("/tmp/sample", [
        "/tmp/notsample",
        "/tmp/somethingelse"
    ]) == "edit /tmp/sample"
