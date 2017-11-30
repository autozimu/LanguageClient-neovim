from urllib import request, parse
from os import path
from typing import List, Dict

from .base import Base

WorkspaceSymbolResults = "g:LanguageClient_workspaceSymbolResults"


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
        if not context["is_async"]:
            context["is_async"] = True
            self.vim.funcs.LanguageClient_workspace_symbol({
                "handle": False,
            })
            return []
        elif self.vim.funcs.eval("len({})".format(WorkspaceSymbolResults)) == 0:
            return []

        context["is_async"] = False
        symbols = self.vim.funcs.eval("remove({}, 0)".format(WorkspaceSymbolResults))

        if symbols is None:
            return []

        return self.convert_to_candidates(symbols)
