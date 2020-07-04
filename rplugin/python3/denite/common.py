from typing import List, Dict
from os.path import dirname, relpath
from urllib import request, parse
import sys
from collections import namedtuple

from denite.source.base import Base

sys.path.insert(0, dirname(__file__))

from lsp.protocol import SymbolKind  # isort:skip  # noqa: I100 E402


MAX_FNAME_LEN = 30

_HighlightDefinition = namedtuple("HighlightDefinition", (
    "name",
    're',
    "contained",
    "contains",
    "nextgroup",
    'link',
))


def HighlightDefinition(name,
                        re,
                        contained=False,
                        contains=None,
                        nextgroup=None,
                        link=None):

    return _HighlightDefinition(name,
                                re,
                                contained,
                                contains,
                                nextgroup,
                                link)


SYMBOL_CANDIDATE_HIGHLIGHT_SYNTAX = [
    HighlightDefinition(
        name='location',
        contains=('colon', 'number', 'path'),
        nextgroup='kind',
        re=r'\([^:]\+:\)\?\d\+:\d\+',
    ),
    HighlightDefinition(
        name='path',
        contained=True,
        re=r'[^:]\+',
        link='String',
    ),
    HighlightDefinition(
        name='colon',
        contained=True,
        re=r':',
        link='Comment',
    ),
    HighlightDefinition(
        name='number',
        contained=True,
        re=r'\d\+',
        link='Number',
    ),
    HighlightDefinition(
        name='kind',
        contained=True,
        nextgroup='name',
        re=r'\s\+\[\(\w\|\s\)*\]',
        link='Type',
    ),
]


def uri_to_path(uri: str) -> str:
    return request.url2pathname(parse.urlparse(uri).path)


def highlight_setup(source: Base, syntax: List[HighlightDefinition]) -> None:
    def mangle_name(name: str) -> str:
        if name in ("TOP", "NONE"):
            return name
        if name.startswith("@"):
            return name

        return "{}_{}".format(source.syntax_name, name)

    for hl_def in syntax:
        match = [
            mangle_name(hl_def.name),
            '/{}/'.format(hl_def.re),
            'contained']
        if hl_def.contains:
            match.append("contains=" + ','.join(
                mangle_name(i) for i in hl_def.contains
            ))
        elif hl_def.contains is None:
            match.append("contains=NONE")

        if hl_def.nextgroup is not None:
            match.append("nextgroup=" + mangle_name(hl_def.nextgroup))

        if not hl_def.contained:
            match.append("containedin=" + source.syntax_name)

        source.vim.command('syntax match ' + ' '.join(match))
        if hl_def.link is not None:
            source.vim.command(
                'highlight default link {0}_{1} {2}'.format(
                    source.syntax_name, hl_def.name, hl_def.link))


def convert_symbols_to_candidates(symbols: List[Dict],
                                  bufname: str = None,
                                  pwd: str = None) -> List[Dict]:
    candidates = []
    paths = []
    kinds = []
    max_path_len = 0
    max_kind_len = 0
    for symbol in symbols:
        name = symbol["name"]
        start = symbol["location"]["range"]["start"]
        line = start["line"] + 1
        character = start["character"] + 1
        kinds.append(SymbolKind(symbol.get("kind", 0)).describe())
        if not bufname:
            filepath = uri_to_path(symbol["location"]["uri"])
            if pwd:
                rpath = relpath(filepath, pwd)
                if len(rpath) < len(filepath):
                    filepath = rpath
            disp_path = filepath
            if len(disp_path) > MAX_FNAME_LEN:
                disp_path = "..." + disp_path[-MAX_FNAME_LEN - 3:]
            paths.append("{}:{}:{}".format(disp_path, line, character))
        else:
            filepath = bufname
            paths.append("{}:{}".format(line, character))
        max_path_len = max(max_path_len, len(paths[-1]))
        max_kind_len = max(max_kind_len, len(kinds[-1]))
        candidates.append({
            "word": name,
            "action__path": filepath,
            "action__line": line,
            "action__col": character,
        })

    for candidate, path, kind in zip(candidates, paths, kinds):
        candidate["abbr"] = "{:<{}} [{:^{}}] {}".format(
            path,
            max_path_len,
            kind,
            max_kind_len,
            candidate["word"],
        )

    return candidates
