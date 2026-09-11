//! Per-hat coder CLI config.
//!
//! The coder CLI keeps its deployment URL and session in `$CODER_CONFIG_DIR`
//! (`~/Library/Application Support/coderv2` on macOS, `~/.config/coderv2`
//! elsewhere), and on macOS it puts the session token in the Keychain. Both are
//! one per machine, so a `coder login` in one terminal is the login in every
//! terminal.
//!
//! Each hat points `CODER_CONFIG_DIR` at a directory of its own. Setting it
//! also makes coder keep the token in that directory rather than the Keychain,
//! so one client's login never reaches another client's shell. Nothing is
//! seeded: a copy of the shared login is exactly what this is meant to avoid,
//! so each hat starts logged out.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// A hat's `$CODER_CONFIG_DIR`.
pub fn config_dir(home: &Path, hat: &str) -> PathBuf {
    home.join(".config").join("coderv2").join("hats").join(hat)
}

/// Give this hat its own coder directory, owner-only because it will hold the
/// session token. An existing directory keeps its mode and its contents.
pub fn isolate(home: &Path, hat: &str) -> Result<PathBuf> {
    let dir = config_dir(home, hat);
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&dir)
        .with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hat_gets_its_own_directory() {
        let home = Path::new("/home/t");
        assert_eq!(
            config_dir(home, "acme"),
            PathBuf::from("/home/t/.config/coderv2/hats/acme")
        );
        assert_ne!(config_dir(home, "a"), config_dir(home, "b"));
    }

    #[cfg(unix)]
    #[test]
    fn the_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let dir = isolate(home.path(), "acme").unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn isolating_again_keeps_the_login() {
        let home = tempfile::tempdir().unwrap();
        let dir = isolate(home.path(), "acme").unwrap();
        std::fs::write(dir.join("session"), "token\n").unwrap();

        isolate(home.path(), "acme").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("session")).unwrap(),
            "token\n"
        );
    }
}
