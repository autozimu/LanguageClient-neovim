use lexpr::sexp;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use vil::{Result, Vim};

static VIM: Lazy<Mutex<Vim>> =
    Lazy::new(|| Mutex::new(Vim::new().expect("Failed to create vim client")));

static ROOT: Lazy<PathBuf> = Lazy::new(|| "tests/data/sample-rs".into());

#[track_caller]
fn vim() -> MutexGuard<'static, Vim> {
    VIM.lock().expect("Failed to lock vim client")
}

/// Assert with timeout.
#[track_caller]
fn assert_eq_timeout<T>(f: fn() -> T, expected: T, timeout: Duration) -> Result<()>
where
    T: std::cmp::PartialEq + std::fmt::Debug,
{
    let now = std::time::Instant::now();

    loop {
        let actual = f();
        if actual == expected {
            return Ok(());
        }
        if now.elapsed() > timeout {
            // Get better error message.
            assert_eq!(actual, expected);
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn test_text_document_definition() -> Result<()> {
    vim().edit(&ROOT.join("src/main.rs"))?;
    vim().cursor(3, 22)?;

    let _: i64 = vim().eval(sexp!(
        (vimcall "LanguageClient#textDocument_definition")
    ))?;

    assert_eq_timeout(
        || {
            let curpos = vim().getcurpos().unwrap();
            (curpos.lnum, curpos.col)
        },
        (8, 4),
        Duration::from_secs(5),
    )
}

#[test]
fn test_text_document_hover() -> Result<()> {
    vim().edit(&ROOT.join("src/main.rs"))?;
    vim().cursor(3, 22)?;

    let _: i64 = vim().eval(sexp!(
        (vimcall "LanguageClient#textDocument_hover")
    ))?;

    assert_eq_timeout(
        || vim().getbufline("__LanguageClient__", 1, 1000).unwrap(),
        vec![
            "```rust",
            "sample",
            "```",
            "",
            "```rust",
            "fn greet() -> i32",
            "```",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
        Duration::from_secs(5),
    )
}
