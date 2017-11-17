from . util import (
    join_path, get_rootPath, path_to_uri, uri_to_path, escape,
    get_command_goto_file,
    get_command_add_sign, get_command_delete_sign, get_command_update_signs,
    convert_vim_command_args_to_kwargs, apply_TextEdit)
from .Sign import Sign
from .DiagnosticSeverity import DiagnosticSeverity


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
    sign = Sign(1, DiagnosticSeverity.Error)
    assert get_command_delete_sign(sign, "") == " | execute 'sign unplace 75000 file='"

    sign = Sign(1, DiagnosticSeverity.Warning)
    assert get_command_delete_sign(sign, "") == " | execute 'sign unplace 75001 file='"

    sign = Sign(1, DiagnosticSeverity.Information)
    assert get_command_delete_sign(sign, "") == " | execute 'sign unplace 75002 file='"

    sign = Sign(1, DiagnosticSeverity.Hint)
    assert get_command_delete_sign(sign, "") == " | execute 'sign unplace 75003 file='"


def test_getCommandAddSign():
    sign = Sign(7, DiagnosticSeverity.Error)
    assert (get_command_add_sign(sign, "") ==
            " | execute 'sign place 75024 line=7 name=LanguageClientError file='")

    sign = Sign(7, DiagnosticSeverity.Warning)
    assert (get_command_add_sign(sign, "") ==
            " | execute 'sign place 75025 line=7 name=LanguageClientWarning file='")

    sign = Sign(7, DiagnosticSeverity.Information)
    assert (get_command_add_sign(sign, "") ==
            " | execute 'sign place 75026 line=7 name=LanguageClientInformation file='")

    sign = Sign(7, DiagnosticSeverity.Hint)
    assert (get_command_add_sign(sign, "") ==
            " | execute 'sign place 75027 line=7 name=LanguageClientHint file='")


def test_getCommandUpdateSigns_unique():
    signs = [
        Sign(1, DiagnosticSeverity.Error),
        Sign(3, DiagnosticSeverity.Error),
    ]
    nextSigns = [
        Sign(1, DiagnosticSeverity.Error),
        Sign(2, DiagnosticSeverity.Error),
        Sign(3, DiagnosticSeverity.Error),
    ]
    assert (get_command_update_signs(signs, nextSigns, "") ==
            "echo | execute 'sign place 75004 line=2 name=LanguageClientError file='")


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
