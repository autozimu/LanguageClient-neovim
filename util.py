import os

currPath = os.path.dirname(os.path.abspath(__file__))

def joinPath(part):
    return os.path.join(currPath, part)

def getRootPath(filename: str) -> str:
    if filename.endswith('.rs'):
        return traverseUp(filename, lambda folder:
                os.path.exists(os.path.join(folder, 'Cargo.toml')))
    # TODO: detect for other filetypes
    else:
        return filename

def traverseUp(folder: str, stop) -> str:
    if stop(folder):
        return folder
    else:
        if folder == "/":
            raise Exception('Failed to found root path')
        return traverseUp(os.path.dirname(folder), stop)

def convertToURI(filename: str) -> str:
    return "file://" + filename

def test_convertToURI():
    assert convertToURI("/tmp/sample-rs/src/main.rs") == "file:///tmp/sample-rs/src/main.rs"
