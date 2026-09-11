//! Per-hat k9s config.
//!
//! k9s reads `config.yaml`, `skins/`, aliases, plugins, hotkeys and views from
//! `$K9S_CONFIG_DIR`. Each hat points that at a directory of its own, so one
//! client's plugins and aliases never show up under another. The theme is
//! shared: a hat's directory holds relative symlinks to the managed
//! `config.yaml` and `skins`, so a change in the repo reaches every hat.
//!
//! It also fixes a quieter problem. On macOS k9s ignores `~/.config/k9s`
//! unless `XDG_CONFIG_HOME` is set, so without this the managed theme is never
//! read at all.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// What a hat's directory shares with the managed one.
const SHARED: &[&str] = &["config.yaml", "skins"];

/// The managed k9s config that each hat's directory links back to.
fn shared_dir(home: &Path) -> PathBuf {
    home.join(".config").join("k9s")
}

/// A hat's `$K9S_CONFIG_DIR`.
pub fn config_dir(home: &Path, hat: &str) -> PathBuf {
    shared_dir(home).join("hats").join(hat)
}

/// One symlink in a hat's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Where the link itself goes.
    pub path: PathBuf,
    /// What the link says. Relative, so the tree survives a moved home.
    pub target: PathBuf,
    /// The managed file or directory the link resolves to.
    pub source: PathBuf,
}

/// Every link a hat's directory should hold.
pub fn links(home: &Path, hat: &str) -> Vec<Link> {
    let dir = config_dir(home, hat);
    SHARED
        .iter()
        .map(|name| Link {
            path: dir.join(name),
            target: Path::new("../..").join(name),
            source: shared_dir(home).join(name),
        })
        .collect()
}

/// Give this hat its own k9s directory, linking in whatever of the shared
/// config exists.
///
/// Anything already in the directory is left alone, so a link the user has
/// replaced with a real file stays replaced.
pub fn isolate(home: &Path, hat: &str) -> Result<PathBuf> {
    let dir = config_dir(home, hat);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    for link in links(home, hat) {
        link_once(&link)?;
    }
    Ok(dir)
}

/// Make one link if nothing is at its path and its source exists. Returns
/// whether it did.
pub fn link_once(link: &Link) -> Result<bool> {
    if link.path.symlink_metadata().is_ok() || !link.source.exists() {
        return Ok(false);
    }
    if let Some(parent) = link.path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    match symlink(&link.target, &link.path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e).with_context(|| {
            format!(
                "linking {} to {}",
                link.path.display(),
                link.target.display()
            )
        }),
    }
}

#[cfg(unix)]
fn symlink(target: &Path, path: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, path)
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "per-hat k9s config needs symlinks",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "k9s:\n  ui:\n    skin: mocha\n";

    fn home_with_theme() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        let k9s = home.path().join(".config/k9s");
        std::fs::create_dir_all(k9s.join("skins")).unwrap();
        std::fs::write(k9s.join("config.yaml"), CONFIG).unwrap();
        std::fs::write(k9s.join("skins/mocha.yaml"), "k9s: {}\n").unwrap();
        home
    }

    #[test]
    fn each_hat_gets_its_own_directory() {
        let home = Path::new("/home/t");
        assert_eq!(
            config_dir(home, "acme"),
            PathBuf::from("/home/t/.config/k9s/hats/acme")
        );
        assert_ne!(config_dir(home, "a"), config_dir(home, "b"));
    }

    #[cfg(unix)]
    #[test]
    fn the_theme_is_linked_relatively_and_resolves() {
        let home = home_with_theme();
        let dir = isolate(home.path(), "acme").unwrap();
        assert_eq!(
            std::fs::read_link(dir.join("config.yaml")).unwrap(),
            PathBuf::from("../../config.yaml")
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("config.yaml")).unwrap(),
            CONFIG
        );
        assert!(dir.join("skins/mocha.yaml").is_file());
    }

    #[test]
    fn a_missing_source_makes_no_link() {
        let home = tempfile::tempdir().unwrap();
        let dir = isolate(home.path(), "solo").unwrap();
        assert!(dir.is_dir());
        assert!(dir.join("config.yaml").symlink_metadata().is_err());
        assert!(dir.join("skins").symlink_metadata().is_err());
    }

    #[test]
    fn an_existing_file_is_never_replaced() {
        let home = home_with_theme();
        let dir = config_dir(home.path(), "acme");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.yaml"), "mine\n").unwrap();

        isolate(home.path(), "acme").unwrap();
        let meta = dir.join("config.yaml").symlink_metadata().unwrap();
        assert!(!meta.file_type().is_symlink());
        assert_eq!(
            std::fs::read_to_string(dir.join("config.yaml")).unwrap(),
            "mine\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn isolating_twice_changes_nothing() {
        let home = home_with_theme();
        isolate(home.path(), "acme").unwrap();
        for link in links(home.path(), "acme") {
            assert!(
                !link_once(&link).unwrap(),
                "{} relinked",
                link.path.display()
            );
        }
    }
}
