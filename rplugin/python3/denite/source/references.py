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

        self.name = 'references'
        self.kind = 'file'

    def convertToCandidate(self, locations) -> List[Dict]:
        candidates = []
        pwd = pathToURI(self.vim.funcs.getcwd())
        for loc in locations:
            uri = loc["uri"]
            filepath = path.relpath(uri, pwd)
            start = loc["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            candidates.append({
                "word": "{}:{}:{}".format(filepath, line, character),
                "action__path": filepath,
                "action__line": line,
                "action__col": character,
            })

        return candidates

    def gather_candidates(self, context):
        locations = LanguageClient._instance.textDocument_references(sync=True)

        if locations is None:
            return []

        return self.convertToCandidate(locations)
