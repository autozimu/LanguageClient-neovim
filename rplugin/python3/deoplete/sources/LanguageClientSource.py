from .base import Base
from os import path
import sys
LanguageClientPath = path.dirname(path.dirname(path.dirname(
    path.realpath(__file__))))
# TODO: use relative path.
sys.path.append(LanguageClientPath)
from LanguageClient import LanguageClient  # noqa: E402


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'LanguageClient'
        self.mark = '[LC]'
        self.rank = 200
        self.filetypes = list(
                self.vim.eval("g:LanguageClient_serverCommands").keys())

    def convertToDeopleteCandidate(self, item):
        cand = {"word": item["label"]}
        if "kind" in item:
            cand["kind"] = item["kind"]
        if "detail" in item:
            cand["info"] = item["detail"]
        return cand

    def gather_candidates(self, context):
        args = {}
        args["line"] = context["position"][1] - 1
        args["character"] = context["position"][2] - 1
        return [self.convertToDeopleteCandidate(item)
                for item in
                LanguageClient._instance.textDocument_completion([args])]
