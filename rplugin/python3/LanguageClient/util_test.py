from . util import joinPath, getRootPath, pathToURI, uriToPath, escape


def test_getRootPath():
    assert (getRootPath(joinPath("tests/sample-rs/src/main.rs"))
            == joinPath("tests/sample-rs"))


def test_pathToURI():
    assert (pathToURI("/tmp/sample-rs/src/main.rs") ==
            "file:///tmp/sample-rs/src/main.rs")


def testUriToPath():
    assert (uriToPath("file:///tmp/sample-rs/src/main.rs") ==
            "/tmp/sample-rs/src/main.rs")


def test_escape():
    assert escape("my' precious") == "my'' precious"
