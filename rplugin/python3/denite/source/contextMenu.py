from typing import List, Dict
from .base import Base


DeniteOverrides = {
    'Code Action': 'codeAction',
    'Document Symbol': 'documentSymbol',
    'Workspace Symbol': 'workspaceSymbol',
    'References': 'references',
}


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'contextMenu'
        self.kind = 'command'

    def convert_to_candidate(self, item):
        if item in DeniteOverrides:
            cmd = 'call denite#start([{{"name": "{}", "args": []}}])'.format(
                DeniteOverrides[item])
        else:
            cmd = ('call '
                   'LanguageClient_handleContextMenuItem("{}")'.format(item))
        return {
            "word": item,
            'action__command': cmd,
        }

    def gather_candidates(self, context: Dict) -> List[Dict]:
        result = self.vim.funcs.LanguageClient_contextMenuItems() or {}
        return [self.convert_to_candidate(key) for key in result.keys()]
