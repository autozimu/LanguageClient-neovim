import os
import time
from urllib import parse
from pathlib import Path
from . logger import logger

currPath = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def joinPath(part):
    return os.path.join(currPath, part)


def getRootPath(filepath: str) -> str:
    if filepath.endswith('.rs'):
        return traverseUp(
            filepath,
            lambda folder: os.path.exists(os.path.join(folder, 'Cargo.toml')))
    # TODO: detect for other filetypes
    else:
        return filepath


def traverseUp(folder: str, stop) -> str:
    if stop(folder):
        return folder
    else:
        if folder == "/":
            raise Exception('Failed to found root path')
        return traverseUp(os.path.dirname(folder), stop)


def pathToURI(filepath: str) -> str:
    return Path(filepath).as_uri()


def uriToPath(uri: str) -> str:
    return parse.urlparse(uri).path


def escape(string: str) -> str:
    return string.replace("'", "''")

def retry(span, count, condition):
    if count > 0 and condition():
        logger.info("retrying...")
        time.sleep(span)
        count -= 1
