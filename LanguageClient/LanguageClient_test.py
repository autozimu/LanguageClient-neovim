import time
import neovim
import pytest

from . util import joinPath

NVIM_LISTEN_ADDRESS = "/tmp/nvim-LanguageClient-IntegrationTest"
PROJECT_ROOT_PATH = joinPath("tests/sample-rs")
MAINRS_PATH = joinPath("tests/sample-rs/src/main.rs")


@pytest.fixture(scope="module")
def nvim() -> neovim.Nvim:
    nvim = neovim.attach('socket', path=NVIM_LISTEN_ADDRESS)
    time.sleep(0.1)
    nvim.command("edit! {}".format(MAINRS_PATH))
    nvim.command("LanguageClientStart")
    nvim.call("LanguageClient_initialize")
    nvim.call("LanguageClient_textDocument_didOpen")
    time.sleep(5)
    return nvim


def test_fixture(nvim):
    pass


def test_textDocument_hover(nvim):
    nvim.command("normal! 9G23|")
    nvim.command("redir => g:echo")
    nvim.call("LanguageClient_textDocument_hover")
    time.sleep(0.5)
    nvim.command("redir END")
    assert nvim.eval("g:echo").strip() == "fn () -> i32"


def test_textDocument_definition(nvim):
    nvim.command("normal! 9G23|")
    nvim.call("LanguageClient_textDocument_definition")
    time.sleep(0.2)
    _, line, character, _ = nvim.eval("getpos('.')")
    assert [line, character] == [3, 4]


def test_textDocument_rename(nvim):
    bufferContent = str.join("\n", nvim.eval("getline(1, '$')"))
    nvim.command("normal! 9G23|")
    nvim.call("LanguageClient_textDocument_rename", {"newName": "hello"})
    time.sleep(0.1)
    updatedBufferContent = str.join("\n", nvim.eval("getline(1, '$')"))
    assert updatedBufferContent == bufferContent.replace("greet", "hello")
    nvim.command("edit! {}".format(MAINRS_PATH))


def test_textDocument_documentSymbol(nvim):
    nvim.call("LanguageClient_textDocument_documentSymbol")


def test_textDocument_didChange(nvim):
    nvim.call("setline", 12, "fn greet_again() -> i64 { 7 }")
    nvim.call("setline", 10, "    println!(\"{}\", greet_again());")
    nvim.call("LanguageClient_textDocument_didChange")
    time.sleep(1)
    nvim.command("normal! 10G23|")
    nvim.call("LanguageClient_textDocument_definition")
    time.sleep(0.1)
    _, line, character, _ = nvim.call("getpos", ".")
    assert (line, character) == (12, 4)


def test_textDocument_didSave(nvim):
    nvim.call("LanguageClient_textDocument_didSave")
