from typing import List, Dict

from .base import Base
from os import path
import sys
LanguageClientPath = path.dirname(path.dirname(path.dirname(
    path.realpath(__file__))))
# TODO: use relative path.
sys.path.append(LanguageClientPath)
from LanguageClient import LanguageClient  # noqa: E402


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'documentSymbol'
        self.kind = 'file'

    def convertToCandidate(self, symbols, bufname) -> List[Dict]:
        candidates = []
        for sb in symbols:
            name = sb["name"]
            start = sb["location"]["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            candidates.append({
                "word": "{}:{}:\t{}".format(line, character, name),
                "action__path": bufname,
                "action__line": line,
                "action__col": character,
            })

        return candidates

    def gather_candidates(self, context):
        symbols = LanguageClient._instance.textDocument_documentSymbol(
            sync=True)

        if symbols is None:
            return []

        bufname = self.vim.current.buffer.name

        return self.convertToCandidate(symbols, bufname)
