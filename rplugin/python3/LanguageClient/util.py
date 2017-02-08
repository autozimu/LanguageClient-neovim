import os
from urllib import parse
from pathlib import Path

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


def testUriToPath():
    assert (uriToPath("file:///tmp/sample-rs/src/main.rs") ==
            "/tmp/sample-rs/src/main.rs")


def escape(string: str) -> str:
    return string.replace("'", "''")


def test_escape():
    assert escape("my' precious") == "my'' precious"
