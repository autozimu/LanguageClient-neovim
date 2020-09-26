use lexpr::sexp;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::time::Duration;
use vil::Vim;

static VIM: Lazy<Vim> = Lazy::new(|| Vim::new().expect("Failed to create VIM"));

static ROOT: Lazy<PathBuf> = Lazy::new(|| "tests/data/sample-rs".into());

/// Assert with timeout.
fn assert_eq_timeout<T>(f: fn() -> T, expected: T, timeout: Duration)
where
    T: std::cmp::PartialEq + std::fmt::Debug,
{
    let now = std::time::Instant::now();

    loop {
        let actual = f();
        if actual == expected {
            break;
        }
        if now.elapsed() > timeout {
            // Get better error message.
            assert_eq!(actual, expected);
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn test_text_document_definition() {
    VIM.edit(&ROOT.join("src/main.rs")).unwrap();
    VIM.cursor(3, 22).unwrap();

    let _: String = VIM
        .eval(sexp!(
            (vimcall "LanguageClient#textDocument_definition")
        ))
        .unwrap();

    assert_eq_timeout(
        || {
            let curpos = VIM.getcurpos().unwrap();
            (curpos.lnum, curpos.col)
        },
        (8, 4),
        Duration::from_secs(5),
    );
}
