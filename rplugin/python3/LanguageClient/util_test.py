from . util import (
    joinPath, getRootPath, pathToURI, uriToPath, escape,
    getGotoFileCommand,
    getCommandAddSign, getCommandDeleteSign, getCommandUpdateSigns,
    convertVimCommandArgsToKwargs)
from . Sign import Sign


def test_getRootPath():
    assert (getRootPath(joinPath("tests/sample-rs/src/main.rs"), "rust") ==
            joinPath("tests/sample-rs"))
    assert (getRootPath("does/not/exists", "") == "does/not")


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


def test_getCommandDeleteSign():
    sign = Sign(1, "Error", 1)
    assert getCommandDeleteSign(sign) == " | execute('sign unplace 1')"


def test_getCommandAddSign():
    sign = Sign(1, "Error", 1)
    assert (getCommandAddSign(sign) ==
            " | execute('sign place 1 line=1"
            " name=LanguageClientError buffer=1')")


def test_getCommandUpdateSigns():
    signs = [
        Sign(1, "Error", 1),
        Sign(3, "Error", 1),
    ]
    nextSigns = [
        Sign(1, "Error", 1),
        Sign(2, "Error", 1),
        Sign(3, "Error", 1),
    ]
    assert (getCommandUpdateSigns(signs, nextSigns) ==
            "echo | execute('sign place 2 line=2"
            " name=LanguageClientError buffer=1')")


def test_convertVimCommandArgsToKwargs():
    assert convertVimCommandArgsToKwargs(["rootPath=/tmp"]) == {
        "rootPath": "/tmp"
    }
