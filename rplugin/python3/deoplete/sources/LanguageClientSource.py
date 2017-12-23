import re
from .base import Base


CompleteResults = "g:LanguageClient_completeResults"


def simplify_snippet(snip: str) -> str:
    snip = re.sub(r'(?<!\\)\$(?P<num>\d+)', '<`\g<num>`>', snip)
    return re.sub(r'(?<!\\)\${(?P<num>\d+):(?P<desc>.+?)}',
                  '<`\g<num>:\g<desc>`>', snip)


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = "LanguageClient"
        self.mark = "[LC]"
        self.rank = 1000
        self.filetypes = vim.eval(
            "get(g:, 'LanguageClient_serverCommands', {})").keys()
        self.min_pattern_length = 1
        self.input_pattern = r'(\.|::)\w*'

    def gather_candidates(self, context):
        if not context["is_async"]:
            context["is_async"] = True
            self.vim.funcs.LanguageClient_omniComplete()
            return []
        elif self.vim.funcs.eval("len({})".format(CompleteResults)) == 0:
            return []

        context["is_async"] = False
        result = self.vim.funcs.eval("remove({}, 0)".format(CompleteResults))
        if not isinstance(result, list):
            result = []
        return result
