//! Per-shell AWS config and credentials isolation.
//!
//! `aws configure`, `aws sso login` and `aws configure set` all write to the
//! shared `~/.aws/config` and `~/.aws/credentials`. Re-authenticating in one
//! terminal therefore rewrites state every other terminal is reading, which is
//! the same class of problem as `kubectl config use-context` and gets the same
//! answer: each hat points at its own pair of files, seeded once from the
//! shared ones.
//!
//! This replaces exporting `AWS_PROFILE`. Selecting a named profile inside a
//! shared file only ever changed which section was read; it did nothing about
//! the file being rewritten underneath other shells.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The directory a hat's AWS files live in.
fn dir(home: &Path) -> PathBuf {
    home.join(".aws").join(".hats")
}

/// Where a hat's own `~/.aws/config` lives.
pub fn config_path(home: &Path, hat: &str) -> PathBuf {
    dir(home).join(format!("{hat}.config"))
}

/// Where a hat's own `~/.aws/credentials` lives.
pub fn credentials_path(home: &Path, hat: &str) -> PathBuf {
    dir(home).join(format!("{hat}.credentials"))
}

/// The shared file a per-hat copy is seeded from, with symlinks resolved.
fn shared(home: &Path, name: &str) -> Option<PathBuf> {
    let path = home.join(".aws").join(name);
    let resolved = std::fs::canonicalize(&path).unwrap_or(path);
    resolved.is_file().then_some(resolved)
}

/// Give this hat its own AWS config and credentials, seeding them the first
/// time.
///
/// Returns `(config, credentials)` for `$AWS_CONFIG_FILE` and
/// `$AWS_SHARED_CREDENTIALS_FILE`. Seeding is one-shot, as with kubeconfigs: a
/// later edit to the shared files does not propagate, which is a documented
/// sharp edge rather than a bug.
pub fn isolate(home: &Path, hat: &str) -> Result<(PathBuf, PathBuf)> {
    let d = dir(home);
    std::fs::create_dir_all(&d).with_context(|| format!("creating {}", d.display()))?;
    owner_only_dir(&d)?;

    let config = seed(&config_path(home, hat), shared(home, "config"))?;
    let credentials = seed(&credentials_path(home, hat), shared(home, "credentials"))?;
    Ok((config, credentials))
}

/// Copy `base` to `target` if `target` does not exist yet, else leave it alone.
///
/// A missing shared file is fine: an empty config is valid, and the CLI creates
/// what it needs on first write.
fn seed(target: &Path, base: Option<PathBuf>) -> Result<PathBuf> {
    if target.exists() {
        return Ok(target.to_path_buf());
    }
    match base {
        Some(b) => {
            std::fs::copy(&b, target)
                .with_context(|| format!("seeding {} from {}", target.display(), b.display()))?;
        }
        None => {
            std::fs::write(target, "")
                .with_context(|| format!("creating {}", target.display()))?;
        }
    }
    owner_only(target)?;
    Ok(target.to_path_buf())
}

#[cfg(unix)]
fn owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting 0600 on {}", path.display()))
}

#[cfg(not(unix))]
fn owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn owner_only_dir(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("setting 0700 on {}", path.display()))
}

#[cfg(not(unix))]
fn owner_only_dir(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "[default]\nregion = eu-west-2\n";
    const CREDS: &str = "[default]\naws_access_key_id = AKIAEXAMPLE\n";

    fn home_with_shared() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let aws = dir.path().join(".aws");
        std::fs::create_dir_all(&aws).unwrap();
        std::fs::write(aws.join("config"), CONFIG).unwrap();
        std::fs::write(aws.join("credentials"), CREDS).unwrap();
        dir
    }

    #[test]
    fn each_hat_gets_its_own_pair_of_paths() {
        let home = Path::new("/home/t");
        assert_eq!(
            config_path(home, "acme"),
            PathBuf::from("/home/t/.aws/.hats/acme.config")
        );
        assert_eq!(
            credentials_path(home, "acme"),
            PathBuf::from("/home/t/.aws/.hats/acme.credentials")
        );
        assert_ne!(config_path(home, "a"), config_path(home, "b"));
    }

    #[test]
    fn the_first_use_seeds_from_the_shared_files() {
        let home = home_with_shared();
        let (cfg, creds) = isolate(home.path(), "acme").unwrap();
        assert_eq!(std::fs::read_to_string(cfg).unwrap(), CONFIG);
        assert_eq!(std::fs::read_to_string(creds).unwrap(), CREDS);
    }

    /// The property the whole design rests on: an `aws sso login` in one hat
    /// must not touch another's files, nor the shared ones.
    #[test]
    fn two_hats_cannot_see_each_others_writes() {
        let home = home_with_shared();
        let (a, _) = isolate(home.path(), "alpha").unwrap();
        let (b, _) = isolate(home.path(), "beta").unwrap();
        assert_ne!(a, b);

        std::fs::write(&a, "[default]\nregion = us-east-1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&b).unwrap(), CONFIG);
        assert_eq!(
            std::fs::read_to_string(home.path().join(".aws/config")).unwrap(),
            CONFIG,
            "the shared config must never drift"
        );
    }

    #[test]
    fn seeding_happens_once_and_never_clobbers_later_edits() {
        let home = home_with_shared();
        let (cfg, _) = isolate(home.path(), "acme").unwrap();
        std::fs::write(&cfg, "edited\n").unwrap();

        let (again, _) = isolate(home.path(), "acme").unwrap();
        assert_eq!(again, cfg);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "edited\n");
    }

    #[test]
    fn a_machine_with_no_aws_files_still_gets_usable_paths() {
        let home = tempfile::tempdir().unwrap();
        let (cfg, creds) = isolate(home.path(), "solo").unwrap();
        assert!(cfg.is_file() && creds.is_file());
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "");
    }

    #[test]
    fn a_symlinked_shared_file_is_resolved_before_copying() {
        let home = tempfile::tempdir().unwrap();
        let aws = home.path().join(".aws");
        std::fs::create_dir_all(&aws).unwrap();
        std::fs::write(aws.join("config.real"), CONFIG).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(aws.join("config.real"), aws.join("config")).unwrap();

        let (cfg, _) = isolate(home.path(), "linked").unwrap();
        assert!(!cfg.is_symlink(), "the copy must be a real file");
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), CONFIG);
    }

    #[cfg(unix)]
    #[test]
    fn the_copies_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let home = home_with_shared();
        let (cfg, creds) = isolate(home.path(), "acme").unwrap();
        for p in [cfg, creds] {
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{} must be owner-only", p.display());
        }
        let mode = std::fs::metadata(dir(home.path()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }
}
