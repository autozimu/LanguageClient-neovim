fn main() {
    let git_hash = option_env!("TRAVIS_COMMIT")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let stdout = std::process::Command::new("git")
                .arg("rev-parse")
                .arg("HEAD")
                .output()
                .expect("Failed to get git commit SHA")
                .stdout;
            String::from_utf8(stdout).expect("Failed to construct string")
        });

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
