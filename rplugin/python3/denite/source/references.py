import sys
from os import path
from typing import List, Dict

from .base import Base

LanguageClientPath = path.dirname(path.dirname(path.dirname(
    path.realpath(__file__))))
# TODO: use relative path.
sys.path.append(LanguageClientPath)
from LanguageClient import LanguageClient, path_to_uri  # noqa: E402

GREP_HEADER_SYNTAX = (
    'syntax match deniteSource_grepHeader '
    r'/\v[^:]*:\d+(:\d+)? / '
    'contained keepend')

GREP_FILE_SYNTAX = (
    'syntax match deniteSource_grepFile '
    r'/[^:]*:/ '
    'contained containedin=deniteSource_grepHeader '
    'nextgroup=deniteSource_grepLineNR')
GREP_FILE_HIGHLIGHT = 'highlight default link deniteSource_grepFile Comment'

GREP_LINE_SYNTAX = (
    'syntax match deniteSource_grepLineNR '
    r'/\d\+\(:\d\+\)\?/ '
    'contained containedin=deniteSource_grepHeader')
GREP_LINE_HIGHLIGHT = 'highlight default link deniteSource_grepLineNR LineNR'

GREP_PATTERNS_HIGHLIGHT = 'highlight default link deniteGrepPatterns Function'


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)
        self.vim = vim

        self.name = 'references'
        self.kind = 'file'

    def define_syntax(self):
        self.vim.command(
                'syntax region ' + self.syntax_name + ' start=// end=/$/ '
                'contains=deniteSource_grepHeader,deniteMatchedRange contained')
        # TODO: make this match the 'range' on each location
        # self.vim.command(
        #         'syntax match deniteGrepPatterns ' +
        #         r'/%s/ ' % r'\|'.join(util.regex_convert_str_vim(pattern)
        #             for pattern in self.context['__patterns']) +
        #         'contained containedin=' + self.syntax_name)

    def highlight(self):
        self.vim.command(GREP_HEADER_SYNTAX)
        self.vim.command(GREP_FILE_SYNTAX)
        self.vim.command(GREP_FILE_HIGHLIGHT)
        self.vim.command(GREP_LINE_SYNTAX)
        self.vim.command(GREP_LINE_HIGHLIGHT)
        self.vim.command(GREP_PATTERNS_HIGHLIGHT)

    def convert_to_candidates(self, locations: List[Dict]) -> List[Dict]:
        candidates = []
        pwd = path_to_uri(self.vim.funcs.getcwd())
        for loc in locations:
            uri = loc["uri"]
            filepath = path.relpath(uri, pwd)
            start = loc["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            text = loc["text"]
            output = '{0}:{1}{2} {3}'.format(
                filepath,
                line,
                (':' + str(character) if character != 0 else ''),
                text)
            candidates.append({
                "word": output,
                "abbr": output,
                "action__path": filepath,
                "action__line": line,
                "action__col": character,
            })

        return candidates

    def gather_candidates(self, context):
        locations = LanguageClient._instance.textDocument_references(handle=False)

        if locations is None:
            return []

        return self.convert_to_candidates(locations)
