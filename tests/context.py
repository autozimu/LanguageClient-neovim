import os
import sys
sys.path.insert(0, os.path.abspath('..'))

from LanguageClient import LanguageClient, getRootPath

currPath = os.path.dirname(os.path.abspath(__file__))

def joinPath(part):
    return os.path.join(currPath, part)
