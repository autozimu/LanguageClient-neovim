import json
import time
import threading
from typing import Callable

import neovim
import pytest

from .util import join_path
from .state import state, update_state

threading.current_thread().name = "Test"

NVIM_LISTEN_ADDRESS = "/tmp/nvim-LanguageClient-IntegrationTest"
PATH_MAINRS = join_path("tests/sample-rs/src/main.rs")
PATH_LIBSRS = join_path("tests/sample-rs/src/libs.rs")


def retry(predicate: Callable[[], bool],
          sleep_time=0.1, max_retry=100) -> None:
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
    update_state({
        "nvim": nvim,
    })
    nvim.command("edit! {}".format(PATH_MAINRS))
    time.sleep(0.5)
    nvim.funcs.LanguageClient_setLoggingLevel("DEBUG")
    nvim.command("LanguageClientStart")
    time.sleep(5)
    assert nvim.funcs.LanguageClient_alive()
    # Sync with plugin host state.
    update_state(json.loads(nvim.funcs.LanguageClient_getState()))
    return nvim


def test_fixture(nvim):
    pass


def test_get_selectionUI(nvim):
    assert state["selectionUI"] == "location-list"


def test_textDocument_hover(nvim):
    nvim.funcs.cursor(3, 23)

    def predicate():
        nvim.command("redir => g:echo")
        nvim.funcs.LanguageClient_textDocument_hover()
        time.sleep(0.2)
        nvim.command("redir END")
        return "fn () -> i32" in nvim.vars.get("echo")

    retry(predicate)
    assert "fn () -> i32" in nvim.vars.get("echo")


def test_textDocument_definition(nvim):
    nvim.funcs.cursor(3, 23)

    def predicate():
        nvim.funcs.LanguageClient_textDocument_definition()
        return nvim.current.window.cursor == [8, 3]

    retry(predicate)
    assert nvim.current.window.cursor == [8, 3]


def test_textDocument_rename(nvim):
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
    time.sleep(1)
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
    nvim.funcs.cursor(8, 4)
    nvim.input("iabc")
    time.sleep(2)
    nvim.funcs.LanguageClient_textDocument_references()
    expect = ["fn abcgreet() -> i32 {"]

    def predicate():
        return [location["text"] for location in nvim.funcs.getloclist(0)] == expect

    retry(predicate)
    assert [location["text"] for location in nvim.funcs.getloclist(0)] == expect
    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_didChange(nvim):
    nvim.funcs.setline(12, "fn greet_again() -> i64 { 7 }")
    nvim.funcs.setline(4, "    println!(\"{}\", greet_again());")
    time.sleep(2)
    nvim.funcs.cursor(4, 23)
    nvim.funcs.LanguageClient_textDocument_definition()

    def predicate():
        return nvim.current.window.cursor == [12, 3]

    retry(predicate)
    assert nvim.current.window.cursor == [12, 3]
    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_throttleChange(nvim):
    pass


def test_textDocument_didClose(nvim):
    nvim.funcs.LanguageClient_textDocument_didClose()
