from .base import Base


COMPLETE_OUTPUTS = "g:LanguageClient_omniCompleteResults"


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = "LanguageClient"
        self.mark = "[LC]"
        self.rank = 1000
        self.min_pattern_length = 1
        self.filetypes = vim.eval(
            "get(g:, 'LanguageClient_serverCommands', {})").keys()
        self.input_pattern = r'(\.|::|->)\w*$'

    def gather_candidates(self, context):
        if context["is_async"]:
            outputs = self.vim.eval(COMPLETE_OUTPUTS)
            if outputs:
                context["is_async"] = False
                # TODO: error handling.
                candidates = outputs[0].get("result", [])
                # log(str(candidates))
                return candidates
        else:
            context["is_async"] = True
            self.vim.command("let {} = []".format(COMPLETE_OUTPUTS))
            character = (context["complete_position"]
                         + len(context["complete_str"]))
            self.vim.funcs.LanguageClient_omniComplete({
                "character": character,
                "complete_position": context["complete_position"],
            })
        return []
