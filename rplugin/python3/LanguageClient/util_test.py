from . util import (
    joinPath, getRootPath, pathToURI, uriToPath, escape,
    getGotoFileCommand,
    getCommandAddSign, getCommandDeleteSign, getCommandUpdateSigns,
    convertVimCommandArgsToKwargs, apply_TextEdit)
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


def test_uriToPath():
    assert (uriToPath("file:///tmp/sample-rs/src/main.rs") ==
            "/tmp/sample-rs/src/main.rs")


def test_uriToPath_quoted():
    assert (uriToPath("file:///tmp/node_modules/%40types/node/index.d.ts") ==
            "/tmp/node_modules/@types/node/index.d.ts")


def test_escape():
    assert escape("my' precious") == "my'' precious"


def test_getGotoFileCommand():
    assert getGotoFileCommand("/tmp/+some str%nge|name", [
        "/tmp/+some str%nge|name",
        "/tmp/somethingelse"
    ]) == "exe 'buffer ' . fnameescape('/tmp/+some str%nge|name')"

    assert getGotoFileCommand("/tmp/+some str%nge|name", [
        "/tmp/notsample",
        "/tmp/somethingelse"
    ]) == "exe 'edit ' . fnameescape('/tmp/+some str%nge|name')"


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

    assert convertVimCommandArgsToKwargs([]) == {}

    assert convertVimCommandArgsToKwargs(None) == {}


def test_apply_TextEdit():
    text = """fn main() {
0;
}
""".split("\n")
    expectedText = """fn main() {
    0;
}
""".split("\n")
    newText = """fn main() {
    0;
}
"""
    textEdit = {
        "range": {
            "start": {
                "line": 0,
                "character": 0,
            },
            "end": {
                "line": 3,
                "character": 0,
            },
        },
        "newText": newText,
    }
    assert apply_TextEdit(text, textEdit) == expectedText
