/// Build script that captures the short git SHA and exposes it as the
/// `GIT_SHA` compile-time environment variable (used via `env!("GIT_SHA")`).
///
/// Falls back to `"unknown"` when git is not available (e.g. in a bare
/// Docker build context without a `.git` directory).
fn main() {
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=GIT_SHA={sha}");

    // Re-run whenever the checked-out commit or branch changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/");
}
