//! Per-shell Azure CLI isolation.
//!
//! Azure CLI stores its configuration and token cache in `~/.azure`. Running
//! `az login` in one terminal rewrites this shared state, affecting all other
//! terminals. Each hat gets its own Azure config directory to prevent this.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The directory a hat's Azure config lives in.
fn dir(home: &Path) -> PathBuf {
    home.join(".azure").join(".hats")
}

/// Where a hat's own Azure config directory lives.
pub fn config_dir(home: &Path, hat: &str) -> PathBuf {
    dir(home).join(hat)
}

/// The shared Azure config directory a per-hat copy is seeded from.
pub fn shared(home: &Path) -> Option<PathBuf> {
    let path = home.join(".azure");
    if path.is_dir() { Some(path) } else { None }
}

/// Give this hat its own Azure config directory, seeded once from the shared
/// config if present.
///
/// Returns the path for `$AZURE_CONFIG_DIR`.
pub fn isolate(home: &Path, hat: &str) -> Result<PathBuf> {
    let target = config_dir(home, hat);
    if target.exists() {
        return Ok(target);
    }
    seed_config(&target, shared(home))?;
    Ok(target)
}

/// Seed this hat's Azure config directory from the shared one if present.
fn seed_config(target: &Path, shared: Option<PathBuf>) -> Result<()> {
    std::fs::create_dir_all(target).with_context(|| format!("creating {}", target.display()))?;

    // Copy common config files if they exist in the shared directory
    if let Some(shared_path) = shared {
        let config_path = shared_path.join("config");
        if config_path.is_file() {
            std::fs::copy(&config_path, target.join("config"))
                .with_context(|| format!("copying Azure config to {}", target.display()))?;
        }

        let cloud_path = shared_path.join("clouds.config");
        if cloud_path.is_file() {
            std::fs::copy(&cloud_path, target.join("clouds.config"))
                .with_context(|| format!("copying Azure clouds config to {}", target.display()))?;
        }
    }

    owner_only(target)
}

#[cfg(unix)]
fn owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("setting 0700 on {}", path.display()))
}

#[cfg(not(unix))]
fn owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hat_gets_its_own_azure_config_directory() {
        let home = Path::new("/home/t");
        assert_eq!(
            config_dir(home, "acme"),
            PathBuf::from("/home/t/.azure/.hats/acme")
        );
        assert_ne!(config_dir(home, "a"), config_dir(home, "b"));
    }

    #[test]
    fn the_first_use_creates_the_directory() {
        let home = tempfile::tempdir().unwrap();
        let path = isolate(home.path(), "acme").unwrap();
        assert!(path.is_dir());
        assert_eq!(path, config_dir(home.path(), "acme"));
    }

    #[test]
    fn two_hats_cannot_see_each_others_directories() {
        let home = tempfile::tempdir().unwrap();
        let a = isolate(home.path(), "alpha").unwrap();
        let b = isolate(home.path(), "beta").unwrap();
        assert_ne!(a, b);

        std::fs::write(a.join("config"), "[profile]").unwrap();
        assert!(!b.join("config").exists());
    }

    #[test]
    fn seeding_copies_shared_config_when_present() {
        let home = tempfile::tempdir().unwrap();
        let shared = home.path().join(".azure");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("config"), "shared-config").unwrap();

        let path = isolate(home.path(), "seeded").unwrap();
        assert!(path.join("config").is_file());
        assert_eq!(
            std::fs::read_to_string(path.join("config")).unwrap(),
            "shared-config"
        );
    }

    #[test]
    fn seeding_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let path = isolate(home.path(), "acme").unwrap();
        std::fs::write(path.join("config"), "edited").unwrap();

        let again = isolate(home.path(), "acme").unwrap();
        assert_eq!(again, path);
        assert_eq!(
            std::fs::read_to_string(again.join("config")).unwrap(),
            "edited"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let path = isolate(home.path(), "acme").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "directory must be owner-only");
    }
}
