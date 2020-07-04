from typing import List, Dict
from os.path import dirname
import sys

from .base import Base

sys.path.insert(0, dirname(dirname(__file__)))

from common import (  # isort:skip  # noqa: I100 E402
    convert_symbols_to_candidates,
    SYMBOL_CANDIDATE_HIGHLIGHT_SYNTAX,
    highlight_setup,
)


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'documentSymbol'
        self.kind = 'file'

    def highlight(self):
        highlight_setup(self, SYMBOL_CANDIDATE_HIGHLIGHT_SYNTAX)

    def gather_candidates(self, context: Dict) -> List[Dict]:
        bufname = self.vim.current.buffer.name
        result = self.vim.funcs.LanguageClient_runSync(
            'LanguageClient_textDocument_documentSymbol', {}) or []
        return convert_symbols_to_candidates(result, bufname)
