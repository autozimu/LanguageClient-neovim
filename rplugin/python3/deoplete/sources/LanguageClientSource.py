from functools import partial
from .base import Base
from os import path
import sys
import re
LanguageClientPath = path.dirname(path.dirname(path.dirname(
    path.realpath(__file__))))
# TODO: use relative path.
sys.path.append(LanguageClientPath)
from LanguageClient import LanguageClient  # noqa: E402
from LanguageClient import CompletionItemKind  # noqa: E402
from LanguageClient import logger  # noqa: F401


def simplify_snippet(snip: str) -> str:
    return re.sub(r'(?<!\\)\$\d+', '', snip)


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'LanguageClient'
        self.mark = '[LC]'
        self.rank = 1000
        self.filetypes = LanguageClient._instance.serverCommands.keys()
        self.min_pattern_length = 1
        self.input_pattern = r'(\.|::)\w*'

        self.__results = {}
        self.__errors = {}

    def get_complete_position(self, context):
        m = re.search('(?:' + context['keyword_patterns'] + ')*$',
                      context['input'])
        return m.start() if m else -1

    def handleCompletionResult(self, items, contextid):
        self.__results[contextid] = items

    def handleCompletionError(self, error, contextid):
        self.__errors[contextid] = error

    def convertToDeopleteCandidate(self, item):
        word = item.get("insertText", item["label"])
        if "textEdit" in item:
            word = item["textEdit"].get("newText", word)
        if item.get("insertTextFormat", 0) == 2:  # snippet
            word = simplify_snippet(word)
        cand = {"word": word, "abbr": item["label"]}
        if "kind" in item:
            cand["kind"] = '[{}]'.format(CompletionItemKind[item["kind"]])
        if "documentation" in item:
            cand["info"] = item["documentation"]
        if "detail" in item:
            cand["menu"] = item["detail"]
        return cand

    def gather_candidates(self, context):
        languageId = context["filetypes"][0]
        if not LanguageClient._instance.alive(
                languageId=languageId, warn=False):
            return []

        contextid = id(context)
        if contextid in self.__results:
            if contextid in self.__errors:  # got error
                context["is_async"] = False
                del self.__errors[contextid]
                return []
            elif self.__results[contextid] is None:  # no response yet
                context["is_async"] = True
                return []
            else:  # got result
                context["is_async"] = False
                items = self.__results[contextid]
                del self.__results[contextid]
                if isinstance(items, dict):
                    items = items["items"]
                return [self.convertToDeopleteCandidate(item)
                        for item in items]
        else:  # send request
            context["is_async"] = True
            self.__results[contextid] = None

            line = context["position"][1] - 1
            character = context["position"][2] - 1
            cbs = [partial(self.handleCompletionResult, contextid=contextid),
                   partial(self.handleCompletionError, contextid=contextid)]
            LanguageClient._instance.textDocument_completion(
                languageId=languageId, line=line, character=character,
                cbs=cbs)

            return []
