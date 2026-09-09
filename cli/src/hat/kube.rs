//! Per-shell kubeconfig isolation.
//!
//! `kubectl config use-context` writes to whatever `$KUBECONFIG` points at. If
//! that is the shared `~/.kube/config`, a context switch in one terminal
//! silently changes every other terminal, which is the incident the whole
//! system exists to prevent. Each hat therefore gets its own copy, seeded
//! once from the shared file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Where a hat's own kubeconfig lives.
pub fn config_path(home: &Path, hat: &str) -> PathBuf {
    home.join(".kube").join(format!("config.{hat}"))
}

/// The shared config a per-hat copy is seeded from, with symlinks resolved.
///
/// Resolving matters: on this machine `~/.kube/config` is a symlink to a k3s
/// file, and copying the link rather than its target would produce a config
/// that still aliases the shared state.
fn shared_config(home: &Path) -> Option<PathBuf> {
    let path = home.join(".kube").join("config");
    let resolved = std::fs::canonicalize(&path).unwrap_or(path);
    resolved.is_file().then_some(resolved)
}

/// Give this hat its own kubeconfig, seeding it the first time.
///
/// Returns the path to use as `$KUBECONFIG`. Seeding is deliberately one-shot:
/// a later edit to the shared config does not propagate, which is a documented
/// sharp edge rather than a bug. `hats doctor` reports the copies' ages.
pub fn isolate(home: &Path, profile: &str) -> Result<PathBuf> {
    let target = config_path(home, profile);
    if target.exists() {
        return Ok(target);
    }

    let dir = target.parent().expect("config path always has a parent");
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    match shared_config(home) {
        Some(base) => {
            std::fs::copy(&base, &target)
                .with_context(|| format!("seeding {} from {}", target.display(), base.display()))?;
            owner_only(&target)?;
        }
        // No shared config to seed from is fine: kubectl will create the file
        // when it first needs to, and an empty KUBECONFIG target is valid.
        None => {
            std::fs::write(&target, "")
                .with_context(|| format!("creating {}", target.display()))?;
            owner_only(&target)?;
        }
    }
    Ok(target)
}

/// Select a context inside a specific kubeconfig.
///
/// Failure is ignored by design, exactly as the shell version's `|| true` did:
/// a missing context or a machine without kubectl must not break opening a
/// terminal. Returns whether it worked so callers can report it under `-v`.
pub fn use_context(kubeconfig: &Path, context: &str) -> bool {
    which::which("kubectl").is_ok()
        && std::process::Command::new("kubectl")
            .arg("--kubeconfig")
            .arg(kubeconfig)
            .args(["config", "use-context", context])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "apiVersion: v1\nkind: Config\ncurrent-context: shared\n";

    fn home_with_shared_config() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let kube = dir.path().join(".kube");
        std::fs::create_dir_all(&kube).unwrap();
        std::fs::write(kube.join("config"), SAMPLE).unwrap();
        dir
    }

    #[test]
    fn each_profile_gets_its_own_path() {
        let home = Path::new("/home/t");
        assert_eq!(
            config_path(home, "acme"),
            PathBuf::from("/home/t/.kube/config.acme")
        );
        assert_ne!(config_path(home, "a"), config_path(home, "b"));
    }

    #[test]
    fn the_first_use_seeds_from_the_shared_config() {
        let home = home_with_shared_config();
        let path = isolate(home.path(), "acme").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SAMPLE);
    }

    /// The property the whole design rests on: writing to one hat's config
    /// must not touch another's, nor the shared file.
    #[test]
    fn two_profiles_cannot_see_each_others_writes() {
        let home = home_with_shared_config();
        let a = isolate(home.path(), "alpha").unwrap();
        let b = isolate(home.path(), "beta").unwrap();
        assert_ne!(a, b);

        std::fs::write(&a, "current-context: alpha\n").unwrap();
        assert_eq!(std::fs::read_to_string(&b).unwrap(), SAMPLE);
        assert_eq!(
            std::fs::read_to_string(home.path().join(".kube/config")).unwrap(),
            SAMPLE,
            "the shared config must never drift"
        );
    }

    #[test]
    fn seeding_happens_once_and_never_clobbers_later_edits() {
        let home = home_with_shared_config();
        let path = isolate(home.path(), "acme").unwrap();
        std::fs::write(&path, "edited\n").unwrap();

        let again = isolate(home.path(), "acme").unwrap();
        assert_eq!(again, path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited\n");
    }

    #[test]
    fn a_symlinked_shared_config_is_resolved_before_copying() {
        let home = tempfile::tempdir().unwrap();
        let kube = home.path().join(".kube");
        std::fs::create_dir_all(&kube).unwrap();
        std::fs::write(kube.join("k3s.yaml"), SAMPLE).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(kube.join("k3s.yaml"), kube.join("config")).unwrap();

        let path = isolate(home.path(), "k3s").unwrap();
        assert!(!path.is_symlink(), "the copy must be a real file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SAMPLE);
    }

    #[test]
    fn a_machine_with_no_kube_config_still_gets_a_usable_path() {
        let home = tempfile::tempdir().unwrap();
        let path = isolate(home.path(), "solo").unwrap();
        assert!(path.is_file());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }

    #[cfg(unix)]
    #[test]
    fn the_copy_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let home = home_with_shared_config();
        let path = isolate(home.path(), "acme").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn selecting_a_context_that_does_not_exist_fails_quietly() {
        let home = home_with_shared_config();
        let path = isolate(home.path(), "acme").unwrap();
        // Either kubectl is absent or the context is missing; neither may panic
        // or write to the terminal.
        assert!(!use_context(&path, "definitely-not-a-context"));
    }
}
