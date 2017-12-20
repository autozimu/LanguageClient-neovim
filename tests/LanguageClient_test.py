import os
import time
import threading
import neovim
import pytest


threading.current_thread().name = "Test"


NVIM_LISTEN_ADDRESS = "/tmp/nvim-LanguageClient-IntegrationTest"


project_root = os.path.dirname(os.path.abspath(__file__))


def join_path(path: str) -> str:
    """Join path to this project tests root."""
    return os.path.join(project_root, path)


PATH_INDEXJS = join_path("data/sample-js/src/index.js")
PATH_LIBSJS = join_path("data/sample-js/src/libs.js")
print(PATH_INDEXJS)


@pytest.fixture(scope="module")
def nvim() -> neovim.Nvim:
    nvim = neovim.attach("socket", path=NVIM_LISTEN_ADDRESS)
    time.sleep(1)
    nvim.command("edit! {}".format(PATH_INDEXJS))
    time.sleep(3)
    return nvim


def test_fixture(nvim):
    pass


def test_textDocument_hover(nvim):
    nvim.command("edit! {}".format(PATH_INDEXJS))
    time.sleep(1)
    nvim.funcs.cursor(13, 19)
    nvim.command("redir => g:echo")
    nvim.funcs.LanguageClient_textDocument_hover()
    time.sleep(1)
    nvim.command("redir END")
    expect = "function greet(): number"

    assert expect in nvim.vars.get("echo")


def test_textDocument_definition(nvim):
    nvim.command("edit! {}".format(PATH_INDEXJS))
    time.sleep(1)
    nvim.funcs.cursor(13, 19)
    nvim.funcs.LanguageClient_textDocument_definition()
    time.sleep(1)

    assert nvim.current.window.cursor == [7, 9]


def test_textDocument_rename(nvim):
    nvim.command("edit! {}".format(PATH_INDEXJS))
    time.sleep(1)
    expect = [line.replace("greet", "hello") for line in nvim.current.buffer]
    nvim.funcs.cursor(13, 19)
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "hello"})
    time.sleep(1)

    assert nvim.current.buffer[:] == expect

    nvim.command("edit! {}".format(PATH_INDEXJS))


def test_textDocument_rename_multiple_oneline(nvim):
    nvim.command("edit! {}".format(PATH_LIBSJS))
    time.sleep(1)
    nvim.funcs.cursor(7, 11)
    expect = [line.replace("a", "abc") for line in nvim.current.buffer]
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "abc"})
    time.sleep(1)

    assert nvim.current.buffer[:] == expect

    nvim.command("bd!")
    nvim.command("edit! {}".format(PATH_INDEXJS))


def test_textDocument_rename_multiple_files(nvim):
    nvim.command("edit! {}".format(PATH_INDEXJS))
    time.sleep(1)
    nvim.funcs.cursor(20, 5)
    expect = [line.replace("yo", "hello") for line in nvim.current.buffer]
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "hello"})
    time.sleep(1)

    assert nvim.current.buffer[:] == expect

    nvim.command("bd!")
    nvim.command("bd!")
    nvim.command("edit! {}".format(PATH_INDEXJS))


def test_textDocument_documentSymbol(nvim):
    nvim.command("edit! {}".format(PATH_INDEXJS))
    time.sleep(1)
    nvim.funcs.cursor(1, 1)
    nvim.funcs.LanguageClient_textDocument_documentSymbol()
    time.sleep(1)

    assert nvim.funcs.getloclist(0)

    nvim.command("4lnext")

    assert nvim.current.window.cursor == [19, 0]


def test_workspace_symbol(nvim):
    nvim.command("edit! {}".format(PATH_LIBSJS))
    time.sleep(1)
    nvim.funcs.cursor(1, 1)
    nvim.funcs.LanguageClient_workspace_symbol()
    time.sleep(1)

    assert nvim.funcs.getloclist(0)

    nvim.command("5lnext")

    assert nvim.current.window.cursor == [7, 0]


def test_textDocument_references(nvim):
    nvim.command("edit! {}".format(PATH_INDEXJS))
    time.sleep(1)
    nvim.funcs.cursor(7, 12)
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(1)
    expect = ["function greet() {",
              """console.log(greet());"""]

    assert [location["text"] for location in nvim.funcs.getloclist(0)] == expect

    nvim.command("lnext")

    assert nvim.current.window.cursor == [13, 16]


def test_textDocument_references_modified_buffer(nvim):
    nvim.command("edit! {}".format(PATH_INDEXJS))
    time.sleep(1)
    nvim.funcs.cursor(7, 10)
    nvim.input("iabc")
    time.sleep(1)
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(1)
    expect = ["function abcgreet() {"]

    assert [location["text"] for location in nvim.funcs.getloclist(0)] == expect

    nvim.command("edit! {}".format(PATH_INDEXJS))
