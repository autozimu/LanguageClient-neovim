import os, time, subprocess
import neovim
import pytest

from . LanguageClient import LanguageClient
from . util import joinPath

NVIM_LISTEN_ADDRESS = "/tmp/nvim-LanguageClient-IntegrationTest"
PROJECT_ROOT_PATH = joinPath("tests/sample-rs")
MAINRS_PATH = joinPath("tests/sample-rs/src/main.rs")


@pytest.fixture(scope="module")
def nvim() -> neovim.Nvim:
    nvim = neovim.attach('socket', path=NVIM_LISTEN_ADDRESS)
    nvim.command("edit {}".format(MAINRS_PATH))
    return nvim

def test_start(nvim):
    nvim.command("LanguageClientStart")

def test_initialize(nvim):
    nvim.command("call LanguageClient_initialize()")

def test_textDocument_didOpen(nvim):
    nvim.command("call LanguageClient_textDocument_didOpen()")

def test_textDocument_didOpen(nvim):
    nvim.command("call LanguageClient_textDocument_didOpen()")

def test_textDocument_hover(nvim):
    nvim.command('normal! 9G23|')
    nvim.command('call LanguageClient_textDocument_hover()')
    time.sleep(3)
    print(nvim.command_output("messages"))

#     def test_textDocument_hover(self):
#         self.client.textDocument_hover(
#                 [joinPath("tests/sample-rs/src/main.rs"), 8, 22],
#                 lambda sign: assertEqual(sign, 'fn () -> i32'))
#         self.waitForResponse(5)

#     def test_textDocument_definition(self):
#         self.client.textDocument_definition(
#                 [joinPath("tests/sample-rs/src/main.rs"), 8, 22],
#                 lambda loc:  assertEqual(loc, [3, 4]))
#         self.waitForResponse(5)

#     def test_textDocument_rename(self):
#         self.client.textDocument_rename(
#                 [joinPath("tests/sample-rs/src/main.rs"), 8, 22, "hello"]
#                 )
#         # TODO: assert changes
#         self.waitForResponse(5)

#     def test_textDocument_documentSymbol(self):
#         self.client.textDocument_documentSymbol(
#                 [joinPath("tests/sample-rs/src/main.rs")]
#                 )
#         # TODO: assert changes
#         self.waitForResponse(5)
