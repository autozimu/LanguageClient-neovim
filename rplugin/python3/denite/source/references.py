from urllib import request, parse
from os import path
from typing import List, Dict

from .base import Base

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


def uri_to_path(uri: str) -> str:
    return request.url2pathname(parse.urlparse(uri).path)


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)
        self.vim = vim

        self.name = 'references'
        self.kind = 'file'

    def define_syntax(self):
        self.vim.command(
            'syntax region ' + self.syntax_name + ' start=// end=/$/ '
            'contains=deniteSource_grepHeader,deniteMatchedRange'
            ' contained')
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
        pwd = self.vim.funcs.getcwd()
        for loc in locations:
            filepath = uri_to_path(loc["uri"])
            relpath = path.relpath(filepath, pwd)
            start = loc["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            text = loc.get("text", "")
            output = '{0}:{1}{2} {3}'.format(
                relpath,
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
        result = self.vim.funcs.LanguageClient_runSync(
            "LanguageClient#textDocument_references", {}) or []
        return self.convert_to_candidates(result)
