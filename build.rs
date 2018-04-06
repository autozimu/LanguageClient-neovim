use std::process::Command;

fn git_hash() -> String {
    let output = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).into()
}

fn main() {
    let git_hash = option_env!("TRAVIS_COMMIT")
        .map(|s| s.into())
        .unwrap_or_else(|| git_hash());

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
