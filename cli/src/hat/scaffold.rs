//! Per-hat files hats creates once and then leaves alone.
//!
//! Every hat reads a few files of its own: ssh hosts, git settings, AWS and
//! kube configs, k9s and coder directories. None of them belong in the repo
//! and hats never renders into them, but a hat whose file does not exist yet is
//! one more thing to remember to create. `hats apply` and `hats hat create`
//! create the missing ones, and that is the end of hats' involvement: they are
//! not recorded in state, so they are never updated or pruned. The one way out
//! is `hats hat delete`, which moves every file the hat owns, edited or not,
//! into the backups along with the hat itself.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::{aws, coder, git, k9s, kube, ssh};
use crate::config::Config;
use crate::model::ManagedFile;

/// Names that would land a hat's file on one every hat shares.
const RESERVED: &[&str] = &["common"];

/// How a scaffold comes into being.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Written once, owner-only, with this text.
    Text(String),
    /// Seeded exactly as `hats env` would: a copy of the shared file, else
    /// empty.
    AwsConfig(String),
    AwsCredentials(String),
    Kube(String),
    /// A relative symlink into the managed k9s config.
    K9s(k9s::Link),
    /// An empty owner-only directory for this hat's coder login.
    Coder(String),
}

/// One path a hat is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scaffold {
    pub target: PathBuf,
    /// `~/...` rather than the full path.
    pub display: String,
    /// What it will hold, for the plan.
    pub how: String,
    pub kind: Kind,
}

/// Every per-hat path that does not exist yet, sorted.
///
/// `managed` is this plan's expanded file list. It decides two things: ssh
/// files are only scaffolded when hats manages the `~/.ssh/config` that
/// includes them, and a k9s link is only made to something that exists or is
/// about to. A hat that fails to resolve is skipped; `hats lint` reports it.
pub fn wanted(cfg: &Config, home: &Path, managed: &[ManagedFile]) -> Vec<Scaffold> {
    let mut out = Vec::new();
    if manages_ssh(home, managed) {
        out.push(text(home, ssh::common_path(home), ssh::scaffold_text(None)));
    }
    for name in cfg.hat_names() {
        out.extend(per_hat(cfg, home, managed, &name));
    }
    finish(out)
}

/// The paths one hat is missing, and nothing for any other hat or the shared
/// ssh file: `hats hat create` makes a hat's files without doing half an apply.
pub fn for_hat(cfg: &Config, home: &Path, managed: &[ManagedFile], name: &str) -> Vec<Scaffold> {
    finish(per_hat(cfg, home, managed, name))
}

fn per_hat(cfg: &Config, home: &Path, managed: &[ManagedFile], name: &str) -> Vec<Scaffold> {
    let Ok(hat) = cfg.resolve_hat(name) else {
        return Vec::new();
    };
    let will_exist = |p: &Path| p.exists() || managed.iter().any(|f| f.target.starts_with(p));

    let mut out = Vec::new();
    if manages_ssh(home, managed) {
        out.push(text(
            home,
            ssh::config_path(home, name),
            ssh::scaffold_text(Some(name)),
        ));
    }
    out.push(text(
        home,
        git::config_path(home, name),
        git::scaffold_text(name),
    ));
    if hat.aws_isolated() {
        out.push(make(
            home,
            aws::config_path(home, name),
            seeded(home, aws::shared(home, "config")),
            Kind::AwsConfig(name.to_string()),
        ));
        out.push(make(
            home,
            aws::credentials_path(home, name),
            seeded(home, aws::shared(home, "credentials")),
            Kind::AwsCredentials(name.to_string()),
        ));
    }
    if hat.kube_isolated() {
        out.push(make(
            home,
            kube::config_path(home, name),
            seeded(home, kube::shared_config(home)),
            Kind::Kube(name.to_string()),
        ));
    }
    if hat.k9s_isolated() {
        for link in k9s::links(home, name) {
            if will_exist(&link.source) {
                out.push(make(
                    home,
                    link.path.clone(),
                    format!("→ {}", tilde(&link.source, home)),
                    Kind::K9s(link),
                ));
            }
        }
    }
    if hat.coder_isolated() {
        out.push(make(
            home,
            coder::config_dir(home, name),
            "empty, owner-only".into(),
            Kind::Coder(name.to_string()),
        ));
    }
    out
}

/// The per-hat files are only read through the managed skeleton's Include,
/// so without it they would be clutter.
fn manages_ssh(home: &Path, managed: &[ManagedFile]) -> bool {
    managed.iter().any(|f| f.target == ssh::skeleton_path(home))
}

fn make(home: &Path, target: PathBuf, how: String, kind: Kind) -> Scaffold {
    Scaffold {
        display: tilde(&target, home),
        target,
        how,
        kind,
    }
}

fn text(home: &Path, target: PathBuf, text: String) -> Scaffold {
    make(home, target, "comment line".into(), Kind::Text(text))
}

fn seeded(home: &Path, shared: Option<PathBuf>) -> String {
    match shared {
        Some(p) => format!("copy of {}", tilde(&p, home)),
        None => "empty".into(),
    }
}

/// Drop whatever is already there and sort, so plans are stable.
fn finish(mut out: Vec<Scaffold>) -> Vec<Scaffold> {
    out.retain(|s| s.target.symlink_metadata().is_err());
    out.sort_by(|a, b| a.target.cmp(&b.target));
    out
}

/// Create one scaffold. Returns false, having touched nothing, when something
/// is already at the path: it was there all along or has appeared since the
/// plan.
pub fn create(s: &Scaffold, home: &Path) -> Result<bool> {
    if s.target.symlink_metadata().is_ok() {
        return Ok(false);
    }
    match &s.kind {
        Kind::Text(text) => write_once(&s.target, text),
        Kind::AwsConfig(hat) => aws::seed_config(home, hat).map(|_| true),
        Kind::AwsCredentials(hat) => aws::seed_credentials(home, hat).map(|_| true),
        Kind::Kube(hat) => kube::isolate(home, hat).map(|_| true),
        Kind::K9s(link) => k9s::link_once(link),
        Kind::Coder(hat) => coder::isolate(home, hat).map(|_| true),
    }
}

/// Every path that belongs to one hat and to nothing else, sorted, whether or
/// not it exists.
///
/// This ignores the hat's isolation settings on purpose: a hat that turned
/// kube isolation off after its kubeconfig was seeded still owns the copy.
/// The k9s and coder entries are whole directories, so plugins and aliases
/// added by hand, and a coder login, go with them.
pub fn owned_paths(home: &Path, name: &str) -> Vec<PathBuf> {
    let mut out = vec![
        ssh::config_path(home, name),
        git::config_path(home, name),
        aws::config_path(home, name),
        aws::credentials_path(home, name),
        kube::config_path(home, name),
        k9s::config_dir(home, name),
        coder::config_dir(home, name),
    ];
    out.sort();
    out
}

/// Check a name is safe to build a hat's paths from.
///
/// A hat's name becomes part of seven paths, and `hats hat delete` removes
/// them, so anything that could climb out of its directory (a slash, a
/// leading dot) or land on a file every hat shares is refused.
pub fn check_name(name: &str) -> Result<()> {
    let usable = !name.is_empty()
        && name.len() <= 64
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !usable {
        bail!(
            "`{name}` is not a usable hat name: use letters, digits, `-`, `_` and `.`, \
             starting with a letter or digit"
        );
    }
    if RESERVED.contains(&name) {
        bail!("`{name}` is reserved: ~/.ssh/config.d/{name}.conf is the file every hat shares");
    }
    Ok(())
}

/// Move one of a hat's paths into `backup`, mirroring where it lived under
/// `home`. A directory goes whole, symlinks and all. Returns false, having
/// touched nothing, when there is nothing at the path.
pub fn retire(path: &Path, backup: &Path, home: &Path) -> Result<bool> {
    let Ok(meta) = path.symlink_metadata() else {
        return Ok(false);
    };
    let rel = path
        .strip_prefix(home)
        .with_context(|| format!("{} is not under {}", path.display(), home.display()))?;
    let dest = backup.join(rel);
    let parent = dest.parent().context("backup path has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    match std::fs::rename(path, &dest) {
        Ok(()) => Ok(true),
        // A backup directory on another filesystem (HATS_HOME elsewhere)
        // cannot take a rename, so copy, then remove the original.
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            copy_tree(path, &dest)?;
            if meta.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            }
            .with_context(|| format!("removing {}", path.display()))?;
            Ok(true)
        }
        Err(e) => {
            Err(e).with_context(|| format!("moving {} to {}", path.display(), dest.display()))
        }
    }
}

/// Copy a file or a directory tree, recreating symlinks rather than following
/// them.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry = entry.with_context(|| format!("walking {}", from.display()))?;
        let rel = entry
            .path()
            .strip_prefix(from)
            .expect("walked path is always under the root");
        let dest = if rel.as_os_str().is_empty() {
            to.to_path_buf()
        } else {
            to.join(rel)
        };
        let kind = entry.file_type();
        let done = if kind.is_dir() {
            std::fs::create_dir_all(&dest)
        } else if kind.is_symlink() {
            std::fs::read_link(entry.path()).and_then(|target| symlink(&target, &dest))
        } else {
            std::fs::copy(entry.path(), &dest).map(|_| ())
        };
        done.with_context(|| format!("copying {} to {}", entry.path().display(), dest.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, path: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, path)
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "copying a symlink needs unix",
    ))
}

/// Write an owner-only file, but only if nothing is there. `create_new`
/// rather than a check-then-write, so a file that appears in between is never
/// overwritten.
fn write_once(target: &Path, text: &str) -> Result<bool> {
    if let Some(dir) = target.parent() {
        private_dirs(dir)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    match opts.open(target) {
        Ok(mut f) => {
            f.write_all(text.as_bytes())
                .with_context(|| format!("writing {}", target.display()))?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e).with_context(|| format!("creating {}", target.display())),
    }
}

/// Create `dir` and any missing parents as 0700. Existing directories keep
/// their mode.
fn private_dirs(dir: &Path) -> Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .with_context(|| format!("creating {}", dir.display()))
}

/// `~/...` for a path under `home`, else the path as it is.
pub(crate) fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;

    fn managed(home: &Path, rel: &str) -> ManagedFile {
        ManagedFile {
            source: Path::new("/repo/files").join(rel),
            rel: rel.into(),
            target: home.join(rel),
            mode: None,
            dir_mode: None,
            template: false,
            secret: false,
            group: "g".into(),
        }
    }

    fn displays(home: &Path, files: &[ManagedFile]) -> Vec<String> {
        wanted(&config(PROFILES), home, files)
            .into_iter()
            .map(|s| s.display)
            .collect()
    }

    #[test]
    fn every_hat_gets_a_file_for_each_tool_it_uses() {
        let home = tempfile::tempdir().unwrap();
        let files = [
            managed(home.path(), ".ssh/config"),
            managed(home.path(), ".config/k9s/config.yaml"),
        ];
        let got = displays(home.path(), &files);
        for want in [
            "~/.ssh/config.d/common.conf",
            "~/.ssh/config.d/normal.conf",
            "~/.ssh/config.d/plain.conf",
            "~/.gitconfig.d/acme",
            "~/.aws/.hats/acme.config",
            "~/.aws/.hats/acme.credentials",
            "~/.kube/config.acme",
            "~/.config/k9s/hats/acme/config.yaml",
            "~/.config/coderv2/hats/acme",
        ] {
            assert!(got.contains(&want.to_string()), "missing {want}: {got:?}");
        }
        // plain opts out of kube, k9s and coder; skins is neither managed nor
        // present.
        for unwanted in [
            "~/.kube/config.plain",
            "~/.config/k9s/hats/plain/config.yaml",
            "~/.config/coderv2/hats/plain",
            "~/.config/k9s/hats/acme/skins",
        ] {
            assert!(!got.contains(&unwanted.to_string()), "{unwanted}: {got:?}");
        }
        let mut sorted = got.clone();
        sorted.sort();
        assert_eq!(got, sorted, "plans must be stable");
    }

    #[test]
    fn no_managed_ssh_config_means_no_ssh_files() {
        let home = tempfile::tempdir().unwrap();
        let got = displays(home.path(), &[]);
        assert!(!got.iter().any(|d| d.starts_with("~/.ssh")), "{got:?}");
        assert!(got.contains(&"~/.gitconfig.d/normal".to_string()));
    }

    #[test]
    fn a_path_that_already_exists_is_not_wanted() {
        let home = tempfile::tempdir().unwrap();
        let existing = git::config_path(home.path(), "acme");
        std::fs::create_dir_all(existing.parent().unwrap()).unwrap();
        std::fs::write(&existing, "[user]\n").unwrap();

        let got = displays(home.path(), &[]);
        assert!(!got.contains(&"~/.gitconfig.d/acme".to_string()));
        assert!(got.contains(&"~/.gitconfig.d/normal".to_string()));
    }

    #[test]
    fn seeded_files_say_what_they_copy() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".aws")).unwrap();
        std::fs::write(home.path().join(".aws/config"), "[default]\n").unwrap();

        let all = wanted(&config(PROFILES), home.path(), &[]);
        let how = |display: &str| {
            all.iter()
                .find(|s| s.display == display)
                .map(|s| s.how.clone())
                .unwrap()
        };
        assert!(how("~/.aws/.hats/acme.config").starts_with("copy of "));
        assert_eq!(how("~/.aws/.hats/acme.credentials"), "empty");
        assert_eq!(how("~/.gitconfig.d/acme"), "comment line");
    }

    #[cfg(unix)]
    #[test]
    fn a_text_scaffold_is_owner_only_and_written_once() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let s = wanted(&config(PROFILES), home.path(), &[])
            .into_iter()
            .find(|s| s.display == "~/.gitconfig.d/acme")
            .unwrap();

        assert!(create(&s, home.path()).unwrap());
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&s.target), 0o600);
        assert_eq!(mode(s.target.parent().unwrap()), 0o700);

        std::fs::write(&s.target, "edited\n").unwrap();
        assert!(!create(&s, home.path()).unwrap(), "must not recreate");
        assert_eq!(std::fs::read_to_string(&s.target).unwrap(), "edited\n");
    }

    #[test]
    fn one_hat_scaffolds_only_its_own_files() {
        let home = tempfile::tempdir().unwrap();
        let files = [managed(home.path(), ".ssh/config")];
        let got: Vec<String> = for_hat(&config(PROFILES), home.path(), &files, "acme")
            .into_iter()
            .map(|s| s.display)
            .collect();
        assert!(
            got.contains(&"~/.ssh/config.d/acme.conf".to_string()),
            "{got:?}"
        );
        assert!(got.contains(&"~/.kube/config.acme".to_string()), "{got:?}");
        assert!(got.iter().all(|d| d.contains("acme")), "{got:?}");
    }

    /// Whatever apply scaffolds for a hat, delete has to find again.
    #[test]
    fn a_hat_owns_every_path_it_could_be_scaffolded() {
        let home = tempfile::tempdir().unwrap();
        let files = [
            managed(home.path(), ".ssh/config"),
            managed(home.path(), ".config/k9s/config.yaml"),
        ];
        let owned = owned_paths(home.path(), "acme");
        for s in for_hat(&config(PROFILES), home.path(), &files, "acme") {
            assert!(
                owned.iter().any(|o| s.target.starts_with(o)),
                "{} is not owned",
                s.display
            );
        }
        assert!(!owned.contains(&ssh::common_path(home.path())));
    }

    #[test]
    fn names_that_could_escape_or_collide_are_refused() {
        for good in ["acme", "globex-prod", "a.b_c", "2024"] {
            assert!(check_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "../etc",
            "a/b",
            ".hidden",
            "-flag",
            "two words",
            "common",
        ] {
            assert!(check_name(bad).is_err(), "{bad:?} should be refused");
        }
        assert!(check_name(&"x".repeat(65)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn retiring_moves_files_and_whole_directories_into_the_backup() {
        let home = tempfile::tempdir().unwrap();
        let backup = home.path().join(".hats/backups/now");
        let git = git::config_path(home.path(), "acme");
        std::fs::create_dir_all(git.parent().unwrap()).unwrap();
        std::fs::write(&git, "[url]\n").unwrap();
        let k9s = k9s::config_dir(home.path(), "acme");
        std::fs::create_dir_all(&k9s).unwrap();
        std::fs::write(k9s.join("plugins.yaml"), "plugins: {}\n").unwrap();
        std::os::unix::fs::symlink("../../config.yaml", k9s.join("config.yaml")).unwrap();

        assert!(retire(&git, &backup, home.path()).unwrap());
        assert!(retire(&k9s, &backup, home.path()).unwrap());
        assert!(
            !retire(
                &kube::config_path(home.path(), "acme"),
                &backup,
                home.path()
            )
            .unwrap()
        );

        assert!(!git.exists() && !k9s.exists());
        assert_eq!(
            std::fs::read_to_string(backup.join(".gitconfig.d/acme")).unwrap(),
            "[url]\n"
        );
        let kept = backup.join(".config/k9s/hats/acme");
        assert!(kept.join("plugins.yaml").is_file());
        assert_eq!(
            std::fs::read_link(kept.join("config.yaml")).unwrap(),
            PathBuf::from("../../config.yaml")
        );
    }

    #[cfg(unix)]
    #[test]
    fn copying_a_tree_keeps_links_as_links() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        std::fs::create_dir_all(from.join("sub")).unwrap();
        std::fs::write(from.join("sub/f"), "x").unwrap();
        std::os::unix::fs::symlink("sub/f", from.join("link")).unwrap();

        copy_tree(&from, &dir.path().join("to")).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("to/sub/f")).unwrap(),
            "x"
        );
        assert!(dir.path().join("to/link").is_symlink());

        std::fs::write(dir.path().join("file"), "y").unwrap();
        copy_tree(&dir.path().join("file"), &dir.path().join("copy")).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("copy")).unwrap(),
            "y"
        );
    }
}
