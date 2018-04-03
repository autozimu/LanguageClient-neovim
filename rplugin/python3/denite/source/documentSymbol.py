from typing import List, Dict

from .base import Base

DocumentSymbolOutputs = "g:LanguageClient_documentSymbolResults"


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
        if context["is_async"]:
            outputs = self.vim.eval(DocumentSymbolOutputs)
            if len(outputs) != 0:
                context["is_async"] = False
                result = outputs[0].get("result", [])
                bufname = self.vim.current.buffer.name
                return [convert_to_candidate(symbol, bufname) for
                        symbol in result]
        else:
            context["is_async"] = True
            self.vim.command("let {0} = []".format(DocumentSymbolOutputs))
            self.vim.funcs.LanguageClient_textDocument_documentSymbol({
                "handle": False,
            })
        return []
