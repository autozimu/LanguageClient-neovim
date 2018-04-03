from .base import Base
import re


CompleteOutputs = "g:LanguageClient_omniCompleteResults"


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = "LanguageClient"
        self.mark = "[LC]"
        self.rank = 1000
        self.sorters = ["sorter_word"]
        self.min_pattern_length = 1
        self.filetypes = vim.eval(
            "get(g:, 'LanguageClient_serverCommands', {})").keys()
        self.input_pattern += r'(\.|::|->)\w*$'
        self.complete_pos = re.compile(r"\w*$")

    def get_complete_position(self, context):
        m = self.complete_pos.search(context['input'])
        return m.start() if m else -1

    def gather_candidates(self, context):
        if context["is_async"]:
            outputs = self.vim.eval(CompleteOutputs)
            if len(outputs) != 0:
                context["is_async"] = False
                # TODO: error handling.
                return outputs[0].get("result", [])
        else:
            context["is_async"] = True
            self.vim.command("let {0} = []".format(CompleteOutputs))
            self.vim.funcs.LanguageClient_omniComplete({
                "character": context["complete_position"],
            })
        return []
