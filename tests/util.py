from .context import getRootPath, joinPath

def test_getRootPath():
    assert (getRootPath(joinPath("sample-rs/src/main.rs"))
            ==  joinPath("sample-rs"))
