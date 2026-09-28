//! `hats version` — the binary and the commit it was built from.

use anyhow::Result;

use crate::app::App;
use crate::cli::VersionArgs;

/// Crate version baked in at build time.
pub const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Short git SHA baked in by build.rs.
pub const BINARY_SHA: &str = env!("HATS_GIT_SHA");

pub fn run(app: &mut App, args: &VersionArgs) -> Result<()> {
    if args.json {
        app.ui.say(serde_json::to_string(&serde_json::json!({
            "version": BINARY_VERSION,
            "sha": BINARY_SHA,
        }))?);
        return Ok(());
    }

    app.ui.say(format!("hats {BINARY_VERSION} ({BINARY_SHA})"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_binary_version_is_a_real_semver() {
        assert!(semver::Version::parse(BINARY_VERSION).is_ok());
    }
}
