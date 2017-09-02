import re
import sys
from os import path
from typing import Dict

from .base import Base

LanguageClientPath = path.dirname(path.dirname(path.dirname(
    path.realpath(__file__))))
# TODO: use relative path.
sys.path.append(LanguageClientPath)
from LanguageClient import (
    LanguageClient, CompletionItemKind,
    logger, state)  # noqa: E402


def simplify_snippet(snip: str) -> str:
    return re.sub(r'(?<!\\)\$\d+', '', snip)


def convert_to_deoplete_candidate(item: Dict) -> Dict:
    word = None
    if item.get("textEdit") is not None:
        word = item.get("textEdit").get("newText")
    if word is None:
        word = item.get("insertText")
    if word is None:
        word = item.get("label")

    if item.get("insertTextFormat", 0) == 2:  # snippet
        word = simplify_snippet(word)
    cand = {"word": word, "abbr": item["label"]}
    if item.get("kind") is not None:
        cand["kind"] = '[{}]'.format(CompletionItemKind[item["kind"]])
    if item.get("documentation") is not None:
        cand["info"] = item["documentation"]
    if item.get("detail") is not None:
        cand["menu"] = item["detail"]
    return cand


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = "LanguageClient"
        self.mark = "[LC]"
        self.rank = 1000
        self.filetypes = state["serverCommands"].keys()
        self.min_pattern_length = 1
        self.input_pattern = r'(\.|::)\w*'

        logger.info("deoplete LanguageClientSource initialized.")

    def get_complete_position(self, context):
        m = re.search('(?:' + context['keyword_patterns'] + ')*$',
                      context['input'])
        return m.start() if m else -1

    def gather_candidates(self, context):
        # context["is_async"] = True

        languageId = context["filetypes"][0]
        line = context["position"][1] - 1
        character = context["position"][2] - 1

        result = LanguageClient._instance.textDocument_completion(
            languageId=languageId, line=line, character=character)

        if result is None:
            return []
        elif isinstance(result, dict):
            items = result["items"]
        else:
            items = result

        return [convert_to_deoplete_candidate(item) for item in items]
