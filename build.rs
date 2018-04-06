use std::process::Command;

fn main() {
    let output = Command::new("git").args(&["rev-parse", "--short", "HEAD"]).output().unwrap();
    let git_hash = String::from_utf8_lossy(&output.stdout);
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
