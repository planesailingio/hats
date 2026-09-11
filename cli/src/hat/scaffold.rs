//! Per-hat files hats creates once and then leaves alone.
//!
//! Every hat reads a few files of its own: ssh hosts, git settings, AWS and
//! kube configs, a k9s directory. None of them belong in the repo and hats
//! never renders into them, but a hat whose file does not exist yet is one more
//! thing to remember to create. `hats apply` creates the missing ones, and that
//! is the end of hats' involvement: they are not recorded in state, so they are
//! never updated, backed up or pruned, and removing a hat leaves its files
//! where they are.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{aws, git, k9s, kube, ssh};
use crate::config::Config;
use crate::model::ManagedFile;

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
    let will_exist = |p: &Path| p.exists() || managed.iter().any(|f| f.target.starts_with(p));
    let ssh = managed.iter().any(|f| f.target == ssh::skeleton_path(home));
    let make = |target: PathBuf, how: String, kind: Kind| Scaffold {
        display: tilde(&target, home),
        target,
        how,
        kind,
    };
    let text =
        |target: PathBuf, text: String| make(target, "comment line".into(), Kind::Text(text));
    let seeded = |shared: Option<PathBuf>| match shared {
        Some(p) => format!("copy of {}", tilde(&p, home)),
        None => "empty".into(),
    };

    let mut out = Vec::new();
    if ssh {
        out.push(text(ssh::common_path(home), ssh::scaffold_text(None)));
    }
    for name in cfg.hat_names() {
        let Ok(hat) = cfg.resolve_hat(&name) else {
            continue;
        };
        if ssh {
            out.push(text(
                ssh::config_path(home, &name),
                ssh::scaffold_text(Some(&name)),
            ));
        }
        out.push(text(
            git::config_path(home, &name),
            git::scaffold_text(&name),
        ));
        if hat.aws_isolated() {
            out.push(make(
                aws::config_path(home, &name),
                seeded(aws::shared(home, "config")),
                Kind::AwsConfig(name.clone()),
            ));
            out.push(make(
                aws::credentials_path(home, &name),
                seeded(aws::shared(home, "credentials")),
                Kind::AwsCredentials(name.clone()),
            ));
        }
        if hat.kube_isolated() {
            out.push(make(
                kube::config_path(home, &name),
                seeded(kube::shared_config(home)),
                Kind::Kube(name.clone()),
            ));
        }
        if hat.k9s_isolated() {
            for link in k9s::links(home, &name) {
                if will_exist(&link.source) {
                    out.push(make(
                        link.path.clone(),
                        format!("→ {}", tilde(&link.source, home)),
                        Kind::K9s(link),
                    ));
                }
            }
        }
    }

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
    }
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

fn tilde(path: &Path, home: &Path) -> String {
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
        ] {
            assert!(got.contains(&want.to_string()), "missing {want}: {got:?}");
        }
        // plain opts out of kube and k9s; skins is neither managed nor present.
        for unwanted in [
            "~/.kube/config.plain",
            "~/.config/k9s/hats/plain/config.yaml",
            "~/.config/k9s/hats/acme/skins",
        ] {
            assert!(!got.contains(&unwanted.to_string()), "{unwanted}: {got:?}");
        }
        let mut sorted = got.clone();
        sorted.sort();
        assert_eq!(got, sorted, "plans must be stable");
    }

    /// The per-hat files are only read through the managed skeleton's
    /// Include, so without it they would be clutter.
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
}
