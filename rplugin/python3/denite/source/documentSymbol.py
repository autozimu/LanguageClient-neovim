import sys
from os import path
from typing import List, Dict

from .base import Base

LanguageClientPath = path.dirname(path.dirname(path.dirname(
    path.realpath(__file__))))
# TODO: use relative path.
sys.path.append(LanguageClientPath)
from LanguageClient import LanguageClient  # noqa: E402


def convert_to_candidate(symbol: Dict, bufname: str) -> Dict:
    name = symbol["name"]
    start = symbol["location"]["range"]["start"]
    line = start["line"] + 1
    character = start["character"] + 1
    return {
        "word": "{}:{}:\t{}".format(line, character, name),
        "action__path": bufname,
        "action__line": line,
        "action__col": character,
    }


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'documentSymbol'
        self.kind = 'file'

    def gather_candidates(self, context: Dict) -> List[Dict]:
        symbols = LanguageClient._instance.textDocument_documentSymbol(handle=False)

        if symbols is None:
            return []

        bufname = self.vim.current.buffer.name

        return [convert_to_candidate(symbol, bufname)
                for symbol in symbols]
