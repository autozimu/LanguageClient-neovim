import time
import neovim
import pytest

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
    nvim.call("LanguageClient_setLoggingLevel", "INFO")
    nvim.command("LanguageClientStart")
    time.sleep(15)
    assert nvim.call("LanguageClient_isAlive")
    return nvim


def test_fixture(nvim):
    pass


def test_textDocument_hover(nvim):
    nvim.command("normal! 3G23|")
    nvim.command("redir => g:echo")
    nvim.call("LanguageClient_textDocument_hover")
    time.sleep(2)
    nvim.command("redir END")
    assert nvim.eval("g:echo").strip() == "fn () -> i32"


def test_textDocument_definition(nvim):
    nvim.command("normal! 3G23|")
    nvim.call("LanguageClient_textDocument_definition")
    time.sleep(2)
    assert nvim.current.window.cursor == [8, 3]


def test_textDocument_rename(nvim):
    bufferContent = str.join("\n", nvim.current.buffer)
    nvim.command("normal! 3G23|")
    nvim.call("LanguageClient_textDocument_rename", {"newName": "hello"})
    time.sleep(2)
    updatedBufferContent = str.join("\n", nvim.current.buffer)
    assert updatedBufferContent == bufferContent.replace("greet", "hello")
    nvim.command("edit! {}".format(MAINRS_PATH))


def test_textDocument_rename_multiple_files(nvim):
    bufferContent = str.join("\n", nvim.current.buffer)
    nvim.command("normal! 17G6|")
    nvim.call("LanguageClient_textDocument_rename", {"newName": "hello"})
    time.sleep(2)
    updatedBufferContent = str.join("\n", nvim.current.buffer)
    assert updatedBufferContent == bufferContent.replace("yo", "hello")
    nvim.command("bd!")
    nvim.command("bd!")
    nvim.command("edit! {}".format(MAINRS_PATH))


def test_textDocument_documentSymbol(nvim):
    nvim.current.buffer.cursor = [1, 1]
    nvim.call("LanguageClient_textDocument_documentSymbol")
    time.sleep(3)
    nvim.feedkeys("gr")
    time.sleep(1)
    nvim.input("<CR>")
    time.sleep(2)
    assert nvim.current.window.cursor == [8, 3]


def test_workspace_symbol(nvim):
    nvim.current.buffer.cursor = [1, 1]
    # rls does not support this method yet.
    nvim.call("LanguageClient_workspace_symbol")


def test_textDocument_didChange(nvim):
    nvim.call("setline", 12, "fn greet_again() -> i64 { 7 }")
    nvim.call("setline", 4, "    println!(\"{}\", greet_again());")
    time.sleep(10)
    nvim.command("normal! 4G23|")
    nvim.call("LanguageClient_textDocument_definition")
    time.sleep(2)
    assert nvim.current.window.cursor == [12, 3]
    nvim.command("edit! {}".format(MAINRS_PATH))


def test_textDocument_didClose(nvim):
    nvim.call("LanguageClient_textDocument_didClose")


def test_exit(nvim):
    nvim.call("LanguageClient_exit")
