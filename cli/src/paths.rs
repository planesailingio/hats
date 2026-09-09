//! Every path hats owns on the machine, resolved once and passed around.
//!
//! Layout (overridable in full with `HATS_HOME`):
//!
//! ```text
//! ~/.hats/
//! ├── repo/          git clone, checked out at the tag matching this binary
//! ├── config.yaml    machine-local wizard answers: identity, groups, hats
//! ├── secrets.yaml   0600, values fetched from the secrets provider
//! ├── envelope.age   0600, provider credentials encrypted to the YubiKey
//! ├── identity.txt   age-plugin-yubikey identity stub (public)
//! ├── state.yaml     last-applied manifest and hook markers
//! ├── backups/<ts>/  pre-overwrite copies and pruned files
//! └── plans/         saved plans
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Resolved locations of everything under the hats home directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HatsPaths {
    pub root: PathBuf,
    pub repo: PathBuf,
    pub config: PathBuf,
    pub secrets: PathBuf,
    pub envelope: PathBuf,
    pub identity: PathBuf,
    pub state: PathBuf,
    pub backups: PathBuf,
    pub plans: PathBuf,
}

impl HatsPaths {
    /// Derive every path from a hats home directory.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            repo: root.join("repo"),
            config: root.join("config.yaml"),
            secrets: root.join("secrets.yaml"),
            envelope: root.join("envelope.age"),
            identity: root.join("identity.txt"),
            state: root.join("state.yaml"),
            backups: root.join("backups"),
            plans: root.join("plans"),
            root,
        }
    }

    /// `--hats-home`, else `$HATS_HOME`, else `~/.hats`.
    ///
    /// The home directory itself is resolved through `directories`, which
    /// honours `$HOME` on unix, so tests can point a whole run at a tempdir.
    pub fn resolve(override_root: Option<&Path>) -> Result<Self> {
        if let Some(root) = override_root {
            return Ok(Self::with_root(root));
        }
        let home = directories::UserDirs::new()
            .context("could not determine the home directory (is $HOME set?)")?
            .home_dir()
            .to_path_buf();
        Ok(Self::with_root(home.join(".hats")))
    }

    /// Create the directories hats writes into. Files are created on demand.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [&self.root, &self.backups, &self.plans] {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }

    /// True once `hats init` has written a config file.
    pub fn is_initialised(&self) -> bool {
        self.config.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_every_path_from_the_root() {
        let p = HatsPaths::with_root("/tmp/h");
        assert_eq!(p.repo, PathBuf::from("/tmp/h/repo"));
        assert_eq!(p.config, PathBuf::from("/tmp/h/config.yaml"));
        assert_eq!(p.secrets, PathBuf::from("/tmp/h/secrets.yaml"));
        assert_eq!(p.state, PathBuf::from("/tmp/h/state.yaml"));
    }

    #[test]
    fn override_wins_over_home() {
        let p = HatsPaths::resolve(Some(Path::new("/custom"))).unwrap();
        assert_eq!(p.root, PathBuf::from("/custom"));
    }

    #[test]
    fn ensure_dirs_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = HatsPaths::with_root(tmp.path().join("hats"));
        p.ensure_dirs().unwrap();
        p.ensure_dirs().unwrap();
        assert!(p.backups.is_dir());
        assert!(p.plans.is_dir());
        assert!(!p.is_initialised());
    }
}
