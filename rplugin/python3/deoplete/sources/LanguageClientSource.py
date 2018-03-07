from .base import Base


CompleteResults = "g:LanguageClient_completeResults"


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = "LanguageClient"
        self.mark = "[LC]"
        self.rank = 1000
        self.filetypes = vim.eval(
            "get(g:, 'LanguageClient_serverCommands', {})").keys()

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
