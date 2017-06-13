from typing import List, Dict

from .base import Base
from os import path
import sys
LanguageClientPath = path.dirname(path.dirname(path.dirname(
    path.realpath(__file__))))
# TODO: use relative path.
sys.path.append(LanguageClientPath)
from LanguageClient import LanguageClient, pathToURI  # noqa: E402


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)
        self.vim = vim

        self.name = 'workspaceSymbol'
        self.kind = 'file'

    def convertToCandidate(self, symbols) -> List[Dict]:
        candidates = []
        pwd = pathToURI(self.vim.funcs.getcwd())
        for sb in symbols:
            name = sb["name"]
            uri = sb["location"]["uri"]
            filepath = path.relpath(uri, pwd)
            start = sb["location"]["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            candidates.append({
                "word": "{}:{}:{}:\t{}".format(
                    filepath, line, character, name),
                "action__path": filepath,
                "action__line": line,
                "action__col": character,
            })

        return candidates

    def gather_candidates(self, context):
        symbols = LanguageClient._instance.workspace_symbol(sync=True)

        if symbols is None:
            return []

        return self.convertToCandidate(symbols)
