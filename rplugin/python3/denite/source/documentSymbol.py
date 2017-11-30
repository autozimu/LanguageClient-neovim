from typing import List, Dict

from .base import Base

DocumentSymbolResults = "g:LanguageClient_documentSymbolResults"


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
        if not context["is_async"]:
            context["is_async"] = True
            self.vim.funcs.LanguageClient_textDocument_documentSymbol({
                "handle": False,
            })
            return []
        elif self.vim.funcs.eval("len({})".format(DocumentSymbolResults)) == 0:
            return []

        context["is_async"] = False
        symbols = self.vim.funcs.eval("remove({}, 0)".format(DocumentSymbolResults))

        if symbols is None:
            return []

        bufname = self.vim.current.buffer.name

        return [convert_to_candidate(symbol, bufname) for symbol in symbols]
