import time
import neovim
import pytest
from collections import Counter

from . util import joinPath

NVIM_LISTEN_ADDRESS = "/tmp/nvim-LanguageClient-IntegrationTest"
PROJECT_ROOT_PATH = joinPath("tests/sample-rs")
MAINRS_PATH = joinPath("tests/sample-rs/src/main.rs")
LIBRS_PATH = joinPath("tests/sample-rs/src/lib.rs")


@pytest.fixture(scope="module")
def nvim() -> neovim.Nvim:
    nvim = neovim.attach('socket', path=NVIM_LISTEN_ADDRESS)
    time.sleep(0.5)
    nvim.command("edit! {}".format(MAINRS_PATH))
    time.sleep(0.5)
    nvim.funcs.LanguageClient_setLoggingLevel("INFO")
    nvim.command("LanguageClientStart")
    time.sleep(15)
    assert nvim.funcs.LanguageClient_alive()
    return nvim


def test_fixture(nvim):
    pass


def test_textDocument_hover(nvim):
    nvim.command("normal! 3G23|")
    nvim.command("redir => g:echo")
    nvim.funcs.LanguageClient_textDocument_hover()
    time.sleep(2)
    nvim.command("redir END")
    assert nvim.eval("g:echo").strip() == "fn () -> i32"


def test_textDocument_definition(nvim):
    nvim.command("normal! 3G23|")
    nvim.funcs.LanguageClient_textDocument_definition()
    time.sleep(2)
    assert nvim.current.window.cursor == [8, 3]


def test_textDocument_rename(nvim):
    bufferContent = str.join("\n", nvim.current.buffer)
    nvim.command("normal! 3G23|")
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "hello"})
    time.sleep(2)
    updatedBufferContent = str.join("\n", nvim.current.buffer)
    assert updatedBufferContent == bufferContent.replace("greet", "hello")
    nvim.command("edit! {}".format(MAINRS_PATH))


def test_textDocument_rename_multiple_files(nvim):
    bufferContent = str.join("\n", nvim.current.buffer)
    nvim.command("normal! 17G6|")
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "hello"})
    time.sleep(2)
    updatedBufferContent = str.join("\n", nvim.current.buffer)
    assert updatedBufferContent == bufferContent.replace("yo", "hello")
    nvim.command("bd!")
    nvim.command("bd!")
    nvim.command("edit! {}".format(MAINRS_PATH))


def test_textDocument_documentSymbol(nvim):
    nvim.current.window.cursor = [1, 1]
    nvim.funcs.LanguageClient_textDocument_documentSymbol()
    time.sleep(3)
    nvim.feedkeys("gr")
    time.sleep(1)
    nvim.input("<CR>")
    time.sleep(2)
    assert nvim.current.window.cursor == [8, 3]


def test_workspace_symbol(nvim):
    nvim.current.window.cursor = [1, 1]
    # rls does not support this method yet.
    nvim.funcs.LanguageClient_workspace_symbol()


def test_textDocument_references(nvim):
    nvim.current.window.cursor = [8, 4]
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(3)
    nvim.feedkeys("3")
    time.sleep(1)
    nvim.input("<CR>")
    time.sleep(2)
    assert nvim.current.window.cursor == [3, 19]


def test_textDocument_references_locationListContent(nvim):
    nvim.command("let g:LanguageClient_selectionUI=\"location-list\"")
    nvim.command("LanguageClientStart")
    nvim.current.window.cursor = [8, 3]
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(3)
    actualLocationTexts = [location["text"] for location in nvim.call("getloclist", "0")]
    expectedLocationTexts = ["fn greet() -> i32 {\n", "    println!(\"{}\", greet());\n"]
    assert Counter(actualLocationTexts) == Counter(expectedLocationTexts)


def test_textDocument_references_locationListContent_modifiedBuffer(nvim):
    nvim.command("let g:LanguageClient_selectionUI=\"location-list\"")
    nvim.command("LanguageClientStart")
    nvim.current.window.cursor = [8, 3]
    nvim.input('iabc')
    time.sleep(0.5)
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(3)
    actualLocationTexts = [location["text"] for location in nvim.call("getloclist", "0")]
    expectedLocationTexts = ["fn abcgreet() -> i32 {\n"]
    assert Counter(actualLocationTexts) == Counter(expectedLocationTexts)
    nvim.command("edit! {}".format(MAINRS_PATH))


def test_textDocument_didChange(nvim):
    nvim.funcs.setline(12, "fn greet_again() -> i64 { 7 }")
    nvim.funcs.setline(4, "    println!(\"{}\", greet_again());")
    time.sleep(10)
    nvim.command("normal! 4G23|")
    nvim.funcs.LanguageClient_textDocument_definition()
    time.sleep(2)
    assert nvim.current.window.cursor == [12, 3]
    nvim.command("edit! {}".format(MAINRS_PATH))


def test_textDocument_throttleChange(nvim):
    pass


def test_textDocument_didClose(nvim):
    nvim.funcs.LanguageClient_textDocument_didClose()
