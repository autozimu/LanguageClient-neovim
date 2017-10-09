from . util import (
    join_path, get_rootPath, path_to_uri, uri_to_path, escape,
    get_command_goto_file,
    get_command_add_sign, get_command_delete_sign, get_command_update_signs,
    convert_vim_command_args_to_kwargs, apply_TextEdit)
from . Sign import Sign


def test_getRootPath():
    assert (get_rootPath(join_path("tests/sample-rs/src/main.rs"), "rust") ==
            join_path("tests/sample-rs"))
    assert (get_rootPath("does/not/exists", "") == "does/not")


def test_pathToURI():
    assert (path_to_uri("/tmp/sample-rs/src/main.rs") ==
            "file:///tmp/sample-rs/src/main.rs")


def test_pathToURIRelative():
    assert path_to_uri(".") is None


def test_uriToPath():
    assert (uri_to_path("file:///tmp/sample-rs/src/main.rs") ==
            "/tmp/sample-rs/src/main.rs")


def test_uriToPath_quoted():
    assert (uri_to_path("file:///tmp/node_modules/%40types/node/index.d.ts") ==
            "/tmp/node_modules/@types/node/index.d.ts")


def test_escape():
    assert escape("my' precious") == "my'' precious"


def test_getGotoFileCommand():
    assert get_command_goto_file("/tmp/+some str%nge|name", [
        "/tmp/+some str%nge|name",
        "/tmp/somethingelse"
    ], 1, 2) == "exe 'buffer +:call\\ cursor(1,2) ' . fnameescape('/tmp/+some str%nge|name')"

    assert get_command_goto_file("/tmp/+some str%nge|name", [
        "/tmp/notsample",
        "/tmp/somethingelse"
    ], 3, 4) == "exe 'edit +:call\\ cursor(3,4) ' . fnameescape('/tmp/+some str%nge|name')"


def test_getCommandDeleteSign():
    sign = Sign(1, "Error", 1)
    assert get_command_delete_sign(sign) == " | execute('sign unplace 75000 buffer=1')"

    sign = Sign(1, "Hint", 2)
    assert get_command_delete_sign(sign) == " | execute('sign unplace 75001 buffer=2')"

    sign = Sign(1, "Information", 3)
    assert get_command_delete_sign(sign) == " | execute('sign unplace 75002 buffer=3')"

    sign = Sign(1, "Warning", 4)
    assert get_command_delete_sign(sign) == " | execute('sign unplace 75003 buffer=4')"


def test_getCommandAddSign():
    sign = Sign(7, "Error", 4)
    assert (get_command_add_sign(sign) ==
            " | execute('sign place 75024 line=7"
            " name=LanguageClientError buffer=4')")

    sign = Sign(7, "Hint", 3)
    assert (get_command_add_sign(sign) ==
            " | execute('sign place 75025 line=7"
            " name=LanguageClientHint buffer=3')")

    sign = Sign(7, "Information", 2)
    assert (get_command_add_sign(sign) ==
            " | execute('sign place 75026 line=7"
            " name=LanguageClientInformation buffer=2')")

    sign = Sign(7, "Warning", 1)
    assert (get_command_add_sign(sign) ==
            " | execute('sign place 75027 line=7"
            " name=LanguageClientWarning buffer=1')")


def test_getCommandUpdateSigns_unique():
    signs = [
        Sign(1, "Error", 1),
        Sign(3, "Error", 1),
    ]
    nextSigns = [
        Sign(1, "Error", 1),
        Sign(2, "Error", 1),
        Sign(3, "Error", 1),
    ]
    assert (get_command_update_signs(signs, nextSigns) ==
            "echo | execute('sign place 75004 line=2"
            " name=LanguageClientError buffer=1')")


def test_getCommandUpdateSigns_withDuplicates():
    signs = [
        Sign(1, "Error", 1),
        Sign(3, "Error", 1),
        Sign(3, "Error", 1),
        Sign(4, "Error", 1),
        Sign(4, "Error", 1),
    ]

    nextSigns = [
        Sign(1, "Error", 1),
        Sign(1, "Error", 1),  # A duplicate value (1) has been added
        Sign(2, "Error", 1),  # A unique value (2) has been added
                              # A unique value (3) has been removed
        Sign(4, "Error", 1),  # A duplicate value (4) has been removed
    ]

    cmd = get_command_update_signs(signs, nextSigns)
    assert "execute('sign place 75000 line=1 name=LanguageClientError buffer=1')" not in cmd
    assert "execute('sign place 75004 line=2 name=LanguageClientError buffer=1')" in cmd
    assert "execute('sign unplace 75008 buffer=1')" in cmd
    assert "execute('sign unplace 75012 buffer=1')" not in cmd


def test_convertVimCommandArgsToKwargs():
    assert convert_vim_command_args_to_kwargs(["rootPath=/tmp"]) == {
        "rootPath": "/tmp"
    }

    assert convert_vim_command_args_to_kwargs([]) == {}

    assert convert_vim_command_args_to_kwargs(None) == {}


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
