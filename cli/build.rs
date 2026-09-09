//! Embeds the short git SHA so `hats version` can report exactly which commit
//! produced the binary. Falls back to "unknown" outside a git checkout (e.g. a
//! source tarball), which must never fail the build.

use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=HATS_GIT_SHA={sha}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
}
