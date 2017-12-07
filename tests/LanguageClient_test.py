import os
import time
import threading
import neovim
import pytest
from typing import Callable


threading.current_thread().name = "Test"


NVIM_LISTEN_ADDRESS = "/tmp/nvim-LanguageClient-IntegrationTest"


project_root = os.path.dirname(os.path.abspath(__file__))


def join_path(path: str) -> str:
    """Join path to this project tests root."""
    return os.path.join(project_root, path)


PATH_MAINRS = join_path("data/sample-rs/src/main.rs")
PATH_LIBSRS = join_path("data/sample-rs/src/libs.rs")
print(PATH_MAINRS)


def retry(predicate: Callable[[], bool],
          sleep_time=0.5, max_retry=60) -> None:
    """
    Retry until predicate is True or exceeds max_retry times.
    """
    count = 0
    while count < max_retry:
        if predicate():
            return
        else:
            time.sleep(sleep_time)
            count += 1


@pytest.fixture(scope="module")
def nvim() -> neovim.Nvim:
    nvim = neovim.attach("socket", path=NVIM_LISTEN_ADDRESS)
    time.sleep(0.5)
    nvim.command("edit! {}".format(PATH_MAINRS))
    time.sleep(5)
    return nvim


def test_fixture(nvim):
    pass


def test_textDocument_hover(nvim):
    nvim.command("edit! {}".format(PATH_MAINRS))
    nvim.funcs.cursor(3, 23)
    nvim.command("redir => g:echo")

    def predicate():
        nvim.funcs.LanguageClient_textDocument_hover()
        return "fn () -> i32" in nvim.vars.get("echo")
    retry(predicate)
    nvim.command("redir END")
    assert "fn () -> i32" in nvim.vars.get("echo")


def test_textDocument_definition(nvim):
    nvim.command("edit! {}".format(PATH_MAINRS))
    nvim.funcs.cursor(3, 23)

    def predicate():
        nvim.funcs.LanguageClient_textDocument_definition()
        return nvim.current.window.cursor == [8, 3]
    retry(predicate)
    assert nvim.current.window.cursor == [8, 3]


def test_textDocument_rename(nvim):
    nvim.command("edit! {}".format(PATH_MAINRS))
    expect = [line.replace("greet", "hello") for line in nvim.current.buffer]
    nvim.funcs.cursor(3, 23)
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "hello"})

    def predicate():
        return nvim.current.buffer[:] == expect
    retry(predicate)
    assert nvim.current.buffer[:] == expect

    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_rename_multiple_oneline(nvim):
    nvim.command("edit! {}".format(PATH_LIBSRS))
    nvim.funcs.cursor(4, 13)
    expect = [line.replace("a", "abc") for line in nvim.current.buffer]
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "abc"})

    def predicate():
        return nvim.current.buffer[:] == expect

    retry(predicate)
    assert nvim.current.buffer[:] == expect

    nvim.command("bd!")
    nvim.command("edit! {}".format(PATH_MAINRS))
    time.sleep(1)


def test_textDocument_rename_multiple_files(nvim):
    nvim.command("edit! {}".format(PATH_MAINRS))
    nvim.funcs.cursor(17, 6)
    expect = [line.replace("yo", "hello") for line in nvim.current.buffer]
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "hello"})

    def predicate():
        return nvim.current.buffer[:] == expect
    retry(predicate)
    assert nvim.current.buffer[:] == expect

    nvim.command("bd!")
    nvim.command("bd!")
    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_documentSymbol(nvim):
    nvim.funcs.cursor(1, 1)
    nvim.funcs.LanguageClient_textDocument_documentSymbol()

    def predicate():
        return nvim.funcs.getloclist(0)
    retry(predicate)
    assert nvim.funcs.getloclist(0)

    nvim.command("3lnext")

    def predicate():
        return nvim.current.window.cursor == [8, 3]
    retry(predicate)
    assert nvim.current.window.cursor == [8, 3]


def test_workspace_symbol(nvim):
    nvim.funcs.cursor(1, 1)
    # TODO: rls just got support for this.
    nvim.funcs.LanguageClient_workspace_symbol()


def test_textDocument_references(nvim):
    nvim.command("edit! {}".format(PATH_MAINRS))
    nvim.funcs.cursor(8, 4)
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(1)
    expect = ["fn greet() -> i32 {",
              """println!("{}", greet());"""]

    def predicate():
        return [location["text"] for location in nvim.funcs.getloclist(0)] == expect
    retry(predicate)
    assert [location["text"] for location in nvim.funcs.getloclist(0)] == expect

    nvim.command("lnext")

    def predicate():
        return nvim.current.window.cursor == [3, 19]
    retry(predicate)
    assert nvim.current.window.cursor == [3, 19]


def test_textDocument_references_modified_buffer(nvim):
    nvim.command("edit! {}".format(PATH_MAINRS))
    time.sleep(3)
    nvim.funcs.cursor(8, 4)
    nvim.input("iabc")
    time.sleep(3)
    expect = ["fn abcgreet() -> i32 {"]

    def predicate():
        nvim.funcs.LanguageClient_textDocument_references()
        time.sleep(3)
        return [location["text"] for location in nvim.funcs.getloclist(0)] == expect
    retry(predicate)
    assert [location["text"] for location in nvim.funcs.getloclist(0)] == expect

    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_throttleChange(nvim):
    pass


def test_textDocument_didClose(nvim):
    nvim.funcs.LanguageClient_textDocument_didClose()
