from urllib import request, parse
from os import path
from typing import List, Dict

from .base import Base


def uri_to_path(uri: str) -> str:
    return request.url2pathname(parse.urlparse(uri).path)


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)
        self.vim = vim

        self.name = 'workspaceSymbol'
        self.kind = 'file'

    def convert_to_candidates(self, symbols: List[Dict]) -> List[Dict]:
        candidates = []
        pwd = self.vim.funcs.getcwd()
        for sb in symbols:
            name = sb["name"]
            uri = sb["location"]["uri"]
            filepath = uri_to_path(uri)
            relpath = path.relpath(filepath, pwd)
            start = sb["location"]["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            candidates.append({
                "word": "{}:{}:{}:\t{}".format(
                    relpath, line, character, name),
                "action__path": filepath,
                "action__line": line,
                "action__col": character,
            })

        return candidates

    def gather_candidates(self, context):
        result = self.vim.funcs.LanguageClient_runSync(
            'LanguageClient#workspace_symbol',
            {"handle": False}) or []
        return self.convert_to_candidates(result)
