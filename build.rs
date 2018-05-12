use std::fs::read_to_string;

fn main() {
    let git_hash = option_env!("TRAVIS_COMMIT")
        .map(|s| s.into())
        .unwrap_or_else(|| read_to_string(".git/refs/heads/next").unwrap_or_default());

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
