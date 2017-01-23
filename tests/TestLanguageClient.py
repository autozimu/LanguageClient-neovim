import os, time
import neovim
from .context import LanguageClient, getRootPath

def assertEqual(v1, v2):
    if v1 != v2:
        raise Exception('Assertion failed, {} == {}'.format(v1, v2))

class TestLanguageClient():
    @classmethod
    def setup_class(cls):
        nvim = neovim.attach('child', argv=['/usr/bin/env', 'nvim', '--embed'])
        cls.client = LanguageClient(nvim)
        cls.currPath = os.path.dirname(os.path.abspath(__file__))
        cls.client.start()

    def joinPath(self, part):
        return os.path.join(self.currPath, part)

    def test_initialize(self):
        self.client.initialize([self.joinPath("sample-rs")])
        while len(self.client.queue) > 0:
            time.sleep(0.1)

        ## wait for notification
        # time.sleep(300)

    def test_textDocument_hover(self):
        self.client.textDocument_didOpen([
            self.joinPath("sample-rs/src/main.rs")
            ])

        time.sleep(2)

        # textDocument/hover
        self.client.textDocument_hover((self.joinPath("sample-rs/src/main.rs"), 8, 22),
                lambda value: assertEqual(value, 'fn () -> i32'))
        while len(self.client.queue) > 0:
            time.sleep(0.1)

    def test_getRootPath(self):
        assert (getRootPath(self.joinPath("sample-rs/src/main.rs"))
                ==  self.joinPath("sample-rs"))
