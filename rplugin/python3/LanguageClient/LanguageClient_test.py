import json
import time
import threading

import neovim
import pytest

from .util import join_path
from .state import state, update_state
from .LanguageClient import (get_selectionUI)

threading.current_thread().name = "Test"

NVIM_LISTEN_ADDRESS = "/tmp/nvim-LanguageClient-IntegrationTest"
PATH_MAINRS = join_path("tests/sample-rs/src/main.rs")
PATH_LIBRS = join_path("tests/sample-rs/src/lib.rs")


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
    time.sleep(15)
    assert nvim.funcs.LanguageClient_alive()
    # Sync with plugin host state.
    update_state(json.loads(nvim.funcs.LanguageClient_getState()))
    return nvim


def test_fixture(nvim):
    pass


def test_get_selectionUI(nvim):
    assert get_selectionUI() == "fzf"
    assert state["selectionUI"] == "location-list"


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
    buffer_content = str.join("\n", nvim.current.buffer)
    nvim.command("normal! 3G23|")
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "hello"})
    time.sleep(3)
    updated_buffer_content = str.join("\n", nvim.current.buffer)
    assert updated_buffer_content == buffer_content.replace("greet", "hello")
    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_rename_multiple_oneline(nvim):
    nvim.command("edit! {}".format(PATH_LIBRS))
    buffer_content = str.join("\n", nvim.current.buffer[:])
    nvim.command("normal! 4G13|")
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "abc"})
    time.sleep(2)
    updated_buffer_content = str.join("\n", nvim.current.buffer)
    assert updated_buffer_content == buffer_content.replace("a", "abc")
    nvim.command("bd!")
    nvim.command("edit! {}".format(PATH_MAINRS))
    time.sleep(1)


def test_textDocument_rename_multiple_files(nvim):
    nvim.command("edit! {}".format(PATH_MAINRS))
    buffer_content = str.join("\n", nvim.current.buffer)
    nvim.command("normal! 17G6|")
    nvim.funcs.LanguageClient_textDocument_rename({"newName": "hello"})
    time.sleep(2)
    updated_buffer_content = str.join("\n", nvim.current.buffer)
    assert updated_buffer_content == buffer_content.replace("yo", "hello")
    nvim.command("bd!")
    nvim.command("bd!")
    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_documentSymbol(nvim):
    nvim.current.window.cursor = [1, 1]
    nvim.funcs.LanguageClient_textDocument_documentSymbol()
    time.sleep(3)
    nvim.command("3lnext")
    assert nvim.current.window.cursor == [8, 3]


def test_workspace_symbol(nvim):
    nvim.current.window.cursor = [1, 1]
    # rls does not support this method yet.
    nvim.funcs.LanguageClient_workspace_symbol()


def test_textDocument_references(nvim):
    nvim.current.window.cursor = [8, 4]
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(3)
    nvim.command("lnext")
    assert nvim.current.window.cursor == [3, 19]


def test_textDocument_references_locationListContent(nvim):
    nvim.current.window.cursor = [8, 3]
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(3)
    actualLocationTexts = [location["text"] for location
                           in nvim.call("getloclist", "0")]
    expectedLocationTexts = ["fn greet() -> i32 {",
                             "println!(\"{}\", greet());"]
    assert actualLocationTexts == expectedLocationTexts


def test_textDocument_references_locationListContent_modifiedBuffer(nvim):
    nvim.current.window.cursor = [8, 3]
    nvim.input("iabc")
    time.sleep(0.5)
    nvim.funcs.LanguageClient_textDocument_references()
    time.sleep(3)
    actualLocationTexts = [location["text"] for location
                           in nvim.call("getloclist", "0")]
    expectedLocationTexts = ["fn abcgreet() -> i32 {"]
    assert actualLocationTexts == expectedLocationTexts
    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_didChange(nvim):
    nvim.funcs.setline(12, "fn greet_again() -> i64 { 7 }")
    nvim.funcs.setline(4, "    println!(\"{}\", greet_again());")
    time.sleep(10)
    nvim.command("normal! 4G23|")
    nvim.funcs.LanguageClient_textDocument_definition()
    time.sleep(3)
    assert nvim.current.window.cursor == [12, 3]
    nvim.command("edit! {}".format(PATH_MAINRS))


def test_textDocument_throttleChange(nvim):
    pass


def test_textDocument_didClose(nvim):
    nvim.funcs.LanguageClient_textDocument_didClose()
