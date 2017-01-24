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
        nvim = neovim.attach('child', argv=['/usr/bin/env', 'nvim', '--embed'])
        cls.client = LanguageClient(nvim)
        cls.client.start()

        cls.client.initialize([joinPath("tests/sample-rs")])
        while len(cls.client.queue) > 0:
            time.sleep(0.1)

        assert cls.client.capabilities

        cls.client.textDocument_didOpen([
            joinPath("tests/sample-rs/src/main.rs")
            ])

        time.sleep(2)

    def test_textDocument_hover(self):
        self.client.textDocument_hover(
                [joinPath("tests/sample-rs/src/main.rs"), 8, 22],
                lambda sign: assertEqual(sign, 'fn () -> i32'))
        while len(self.client.queue) > 0:
            time.sleep(0.1)

    def test_textDocument_definition(self):
        self.client.textDocument_definition(
                [joinPath("tests/sample-rs/src/main.rs"), 8, 22],
                lambda loc:  assertEqual(loc, [3, 4]))
        while len(self.client.queue) > 0:
            time.sleep(0.1)
