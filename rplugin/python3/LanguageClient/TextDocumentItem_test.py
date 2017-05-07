from . TextDocumentItem import TextDocumentItem


def test_TextDocumentItem():
    textDocumentItem = TextDocumentItem(
        "file:///tmp/sample.rs",
        "rust",
        """

fn greet() -> i32 {
    42
}

fn main() {
    let a = 1;
    println!("{}", greet());

}
""".splitlines())

    newText = """fn greet_again() -> i8 { 423 }

fn greet() -> i32 {
    42
}

fn main() {
    let a = 1;
    println!("{}", greet());
    println!("{}", greet_again());
}
""".splitlines()

    version, changes = textDocumentItem.change(newText)

    assert textDocumentItem.version == 2
    assert textDocumentItem.text == newText
    assert version == 2
    assert len(changes) == 1
