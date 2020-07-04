from os import path
import sys

from .base import Base

sys.path.insert(0, path.dirname(path.dirname(__file__)))

from common import (  # isort:skip  # noqa: I100 E402
    convert_symbols_to_candidates,
    SYMBOL_CANDIDATE_HIGHLIGHT_SYNTAX,
    highlight_setup,
)


class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)
        self.vim = vim

        self.name = 'workspaceSymbol'
        self.kind = 'file'

    def highlight(self):
        highlight_setup(self, SYMBOL_CANDIDATE_HIGHLIGHT_SYNTAX)

    def gather_candidates(self, context):
        context['is_interactive'] = True
        prefix = context['input']
        bufnr = context['bufnr']

        # This a hack to get around the fact that LanguageClient APIs
        # work in the context of the active buffer, when filtering results
        # interactively, the denite buffer is the active buffer and it doesn't
        # have a language server asscosiated with it.
        # We just switch to the buffer that initiated the denite transaction
        # and execute the command from it. This should be changed when we
        # have a better way to run requests out of the buffer.
        # See issue#674
        current_buffer = self.vim.current.buffer.number
        if current_buffer != bufnr:
            self.vim.command("tabedit %")
            self.vim.command(
                "execute 'noautocmd keepalt buffer' {}".format(bufnr))
        result = self.vim.funcs.LanguageClient_runSync(
            'LanguageClient#workspace_symbol', prefix, {}) or []
        if current_buffer != bufnr:
            self.vim.command("tabclose")

        candidates = convert_symbols_to_candidates(
            result,
            pwd=self.vim.funcs.getcwd())

        return candidates
