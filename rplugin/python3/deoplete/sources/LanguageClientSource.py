from .base import Base
from os import path
import sys
LanguageClientPath = path.dirname(path.dirname(path.dirname(path.realpath(__file__))))
# TODO: use relative path.
sys.path.append(LanguageClientPath)
from LanguageClient import LanguageClient

class Source(Base):
    def __init__(self, vim):
        super().__init__(vim)

        self.name = 'LanguageClient'
        self.mark = '[LC]'
        self.debug_enabled = True


    def gather_candidates(self, context):
        return ["Hello", "Yoooo"]

