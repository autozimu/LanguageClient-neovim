from util import joinPath, getRootPath

def test_getRootPath():
    assert (getRootPath(joinPath("tests/sample-rs/src/main.rs"))
            ==  joinPath("tests/sample-rs"))
