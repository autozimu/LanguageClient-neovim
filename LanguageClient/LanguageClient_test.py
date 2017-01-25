import os, time
import neovim

from . LanguageClient import LanguageClient
from . util import joinPath

def assertEqual(v1, v2):
    if v1 != v2:
        raise Exception('Assertion failed, {} == {}'.format(v1, v2))

class TestLanguageClient():
    @classmethod
    def setup_class(cls):
        nvim = neovim.attach('child', argv=[
            '/usr/bin/env', 'nvim', '--embed', '-U', 'NONE'
            ])
        cls.client = LanguageClient(nvim)
        cls.client.start()

        cls.client.initialize([joinPath("tests/sample-rs")])

        cls.client.textDocument_didOpen([
            joinPath("tests/sample-rs/src/main.rs")
            ])

        time.sleep(3)

        assert cls.client.capabilities

    def waitForResponse(self, timeout):
        while len(self.client.queue) > 0 and timeout > 0:
            time.sleep(0.1)
            timeout -= 0.1

        if len(self.client.queue) > 0:
            assert False, "timeout"

    def test_textDocument_hover(self):
        self.client.textDocument_hover(
                [joinPath("tests/sample-rs/src/main.rs"), 8, 22],
                lambda sign: assertEqual(sign, 'fn () -> i32'))
        self.waitForResponse(5)

    def test_textDocument_definition(self):
        self.client.textDocument_definition(
                [joinPath("tests/sample-rs/src/main.rs"), 8, 22],
                lambda loc:  assertEqual(loc, [3, 4]))
        self.waitForResponse(5)

    def test_textDocument_rename(self):
        self.client.textDocument_rename(
                [joinPath("tests/sample-rs/src/main.rs"), 8, 22, "hello"]
                )
        # TODO: assert changes
        self.waitForResponse(5)

    def test_textDocument_documentSymbol(self):
        self.client.textDocument_documentSymbol(
                [joinPath("tests/sample-rs/src/main.rs")]
                )
        # TODO: assert changes
        self.waitForResponse(5)
