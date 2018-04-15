from typing import List, Dict
from .base import Base


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'codeAction'
        self.kind = 'command'

    def gather_candidates(self, context: Dict) -> List[Dict]:
        result = self.vim.funcs.LanguageClient_runSync(
            'LanguageClient_textDocument_codeAction', {}) or []
        return [convert_to_candidate(item) for item in result]


def convert_to_candidate(cmd: Dict) -> Dict:
    cmd_str = '{}: {}'.format(cmd['command'], cmd['title'])
    return {
        'word': cmd_str,
        'action__command': 'call '
        'LanguageClient_FZFSinkCommand("{}")'.format(cmd_str),
    }
