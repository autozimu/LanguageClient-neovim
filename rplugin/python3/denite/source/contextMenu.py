from typing import List, Dict
from .base import Base


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'contextMenu'
        self.kind = 'command'

    def convert_to_candidate(self, item):
        return {
            "word": item,
            'action__command': 'call '
            'LanguageClient_handleContextMenuItem("{}")'.format(item),
        }

    def gather_candidates(self, context: Dict) -> List[Dict]:
        result = self.vim.funcs.LanguageClient_contextMenuItems() or {}
        return [self.convert_to_candidate(key) for key in result.keys()]
