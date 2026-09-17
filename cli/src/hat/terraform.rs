//! Per-shell Terraform CLI config isolation.
//!
//! Terraform keeps per-user CLI configuration in `~/.terraformrc` (and reads
//! the `~/.terraform.d/` directory for `*.tfrc` and `*.tfrc.json` files, which
//! is also where `terraform login` stores its token in
//! `credentials.tfrc.json`). Re-authenticating or editing that shared state in
//! one terminal rewrites settings every other terminal is reading, so each hat
//! points at its own CLI config file via `TF_CLI_CONFIG_FILE`.
//!
//! Setting `TF_CLI_CONFIG_FILE` also makes Terraform skip the shared
//! `~/.terraform.d` directory entirely, so a hat's `credentials`,
//! `credentials_helper`, `plugin_cache_dir` and `provider_installation`
//! settings stand alone.
//!
//! Sharp edge (undocumented anywhere else, noted for honesty): `terraform
//! login` always *writes* its token to `~/.terraform.d/credentials.tfrc.json`,
//! which the path above no longer reads. A hat that wants HCP tokens to live
//! with the hat writes a `credentials` block into its own tfrc (or exports
//! `TF_TOKEN_app_terraform_io`); `terraform login` in that hat then updates
//! the shared file, which the hat ignores.
//!
//! OpenTofu honours `TF_CLI_CONFIG_FILE` as well, in place of both `~/.tofurc`
//! and `~/.terraformrc`, so one per-hat file serves both tools.
//!
//! The file is named `<hat>.tfrc` because Terraform only honours
//! `*.tfrc`-named files in config directories.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The directory a hat's Terraform config file lives in.
fn dir(home: &Path) -> PathBuf {
    home.join(".terraform.d").join(".hats")
}

/// Where a hat's own Terraform CLI config file lives.
pub fn config_path(home: &Path, hat: &str) -> PathBuf {
    dir(home).join(format!("{hat}.tfrc"))
}

/// The shared CLI config file a per-hat copy is seeded from, with symlinks
/// resolved.
///
/// `~/.terraformrc` first, since both tools read it; `~/.tofurc` when that is
/// all the machine has, because OpenTofu honours `TF_CLI_CONFIG_FILE` too and
/// an empty per-hat file would cost it the settings it had outside a hat.
pub fn shared(home: &Path) -> Option<PathBuf> {
    [".terraformrc", ".tofurc"].into_iter().find_map(|name| {
        let path = home.join(name);
        let resolved = std::fs::canonicalize(&path).unwrap_or(path);
        resolved.is_file().then_some(resolved)
    })
}

/// Give this hat its own Terraform CLI config file, created once.
///
/// Returns the path for `$TF_CLI_CONFIG_FILE`. Seeding is one-shot, as with
/// AWS and kube: a later edit to the shared file does not propagate, which is
/// a documented sharp edge rather than a bug.
pub fn isolate(home: &Path, hat: &str) -> Result<PathBuf> {
    let d = dir(home);
    std::fs::create_dir_all(&d).with_context(|| format!("creating {}", d.display()))?;
    owner_only_dir(&d)?;

    let target = config_path(home, hat);
    if !target.exists() {
        seed(&target, shared(home))?;
    }
    Ok(target)
}

/// Copy `base` to `target` if `target` does not exist yet, else leave it
/// alone. A missing shared file is fine: an empty config is valid, and
/// Terraform creates what it needs on first write.
fn seed(target: &Path, base: Option<PathBuf>) -> Result<()> {
    match base {
        Some(b) => {
            std::fs::copy(&b, target)
                .with_context(|| format!("seeding {} from {}", target.display(), b.display()))?;
        }
        None => {
            std::fs::write(target, "").with_context(|| format!("creating {}", target.display()))?;
        }
    }
    owner_only(target)
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

    const TFRC: &str = "# shared tfrc\ndisable_checkpoint = true\n";

    fn home_with_shared() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".terraformrc"), TFRC).unwrap();
        dir
    }

    #[test]
    fn each_hat_gets_its_own_terraform_config_file() {
        let home = Path::new("/home/t");
        assert_eq!(
            config_path(home, "acme"),
            PathBuf::from("/home/t/.terraform.d/.hats/acme.tfrc")
        );
        assert_ne!(config_path(home, "a"), config_path(home, "b"));
    }

    #[test]
    fn the_first_use_seeds_from_the_shared_file() {
        let home = home_with_shared();
        let file = isolate(home.path(), "acme").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), TFRC);
    }

    #[test]
    fn a_machine_with_no_terraformrc_still_gets_a_usable_file() {
        let home = tempfile::tempdir().unwrap();
        let file = isolate(home.path(), "solo").unwrap();
        assert!(file.is_file());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
    }

    #[test]
    fn a_machine_with_only_a_tofurc_seeds_from_that() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".tofurc"), "# tofu\n").unwrap();
        let file = isolate(home.path(), "acme").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# tofu\n");

        // With both present, the file both tools read wins.
        std::fs::write(home.path().join(".terraformrc"), TFRC).unwrap();
        let file = isolate(home.path(), "both").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), TFRC);
    }

    #[test]
    fn two_hats_cannot_see_each_others_writes() {
        let home = home_with_shared();
        let a = isolate(home.path(), "alpha").unwrap();
        let b = isolate(home.path(), "beta").unwrap();
        assert_ne!(a, b);

        std::fs::write(&a, "# alpha\n").unwrap();
        assert_eq!(std::fs::read_to_string(&b).unwrap(), TFRC);
        assert_eq!(
            std::fs::read_to_string(home.path().join(".terraformrc")).unwrap(),
            TFRC,
            "the shared tfrc must never drift"
        );
    }

    #[test]
    fn seeding_happens_once_and_never_clobbers_later_edits() {
        let home = home_with_shared();
        let file = isolate(home.path(), "acme").unwrap();
        std::fs::write(&file, "# edited\n").unwrap();

        let again = isolate(home.path(), "acme").unwrap();
        assert_eq!(again, file);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# edited\n");
    }

    #[test]
    fn a_symlinked_shared_file_is_resolved_before_copying() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("terraformrc.real"), TFRC).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            home.path().join("terraformrc.real"),
            home.path().join(".terraformrc"),
        )
        .unwrap();

        let file = isolate(home.path(), "linked").unwrap();
        assert!(!file.is_symlink(), "the copy must be a real file");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), TFRC);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_and_its_directory_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let home = home_with_shared();
        let file = isolate(home.path(), "acme").unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the tfrc must be owner-only");
        let dir_mode = std::fs::metadata(dir(home.path()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
    }
}
