from .base import Base
import re


CompleteOutputs = "g:LanguageClient_omniCompleteResults"


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = "LanguageClient"
        self.mark = "[LC]"
        self.rank = 1000
        self.min_pattern_length = 0
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
                candidates = outputs[0].get("result", [])
                # log(str(candidates))
                return candidates
        else:
            context["is_async"] = True
            self.vim.command("let {} = []".format(CompleteOutputs))
            self.vim.funcs.LanguageClient_omniComplete({
                "character": context["complete_position"] + len(context["complete_str"]),
            })
        return []


# f = open("/tmp/deoplete.log", "w")


# def log(message):
#     f.writelines([message])
#     f.flush()
