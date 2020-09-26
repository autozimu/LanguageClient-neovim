use once_cell::sync::Lazy;
use std::path::PathBuf;
use vil::Vim;
use lexpr::sexp;

static VIM: Lazy<Vim> = Lazy::new(|| Vim::new().expect("Failed to create VIM"));

static ROOT: Lazy<PathBuf> = Lazy::new(|| "tests/data/sample-rs".into());

#[test]
fn test_text_document_definition() {
    VIM.edit(&ROOT.join("src/main.rs")).unwrap();
    VIM.cursor(3, 22).unwrap();

    let _: String = VIM.eval(
        sexp!(
            (vimcall "LanguageClient#textDocument_definition")
        )
    ).unwrap();

    std::thread::sleep_ms(3000);
    let curpos = VIM.getcurpos().unwrap();
    assert_eq!(curpos.lnum, 8);
    assert_eq!(curpos.col, 4);
}
