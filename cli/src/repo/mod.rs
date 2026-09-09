//! The dotfiles clone under `~/.hats/repo`, and the version lockstep.
//!
//! The binary and the dotfiles content are released from one git tag. The
//! clone is therefore a release artefact, not a working copy: it sits detached
//! at `v{CARGO_PKG_VERSION}`. That is the whole reason `hats update` takes no
//! version argument — there is exactly one right answer, and it is printed on
//! `hats --version`.

pub mod git;

use std::path::{Path, PathBuf};

use crate::error::RepoError;
use git::Git;

/// The version this binary was built as, e.g. "0.1.0".
pub const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Short git SHA of the commit that built this binary.
pub const BINARY_SHA: &str = env!("HATS_GIT_SHA");

/// The tag this binary expects the repo to be checked out at.
pub fn wanted_tag() -> String {
    format!("v{BINARY_VERSION}")
}

/// How the repo's checkout relates to this binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionStatus {
    /// The repo is at the tag this binary expects.
    Match { tag: String },
    /// Homebrew upgraded the binary but `hats update` has not run yet. The
    /// common case, and recoverable in one command.
    RepoBehind {
        binary: String,
        repo: Option<String>,
    },
    /// The repo was moved to a newer tag than the binary: upgrade the binary.
    RepoAhead { binary: String, repo: String },
    /// HEAD is not on any tag, which is normal for a development clone.
    Untagged { binary: String },
    /// No clone at all.
    Missing,
}

impl VersionStatus {
    /// Whether commands that read the repo should proceed without complaint.
    pub fn is_ok(&self) -> bool {
        matches!(self, VersionStatus::Match { .. })
    }

    /// Exit code for `hats update --check`: 0 match, 3 repo behind, 4 binary
    /// behind, 1 anything else.
    pub fn exit_code(&self) -> i32 {
        match self {
            VersionStatus::Match { .. } => 0,
            VersionStatus::RepoBehind { .. } => 3,
            VersionStatus::RepoAhead { .. } => 4,
            VersionStatus::Untagged { .. } | VersionStatus::Missing => 1,
        }
    }

    /// One line the user can act on.
    pub fn summary(&self) -> String {
        match self {
            VersionStatus::Match { tag } => format!("repo and binary are both at {tag}"),
            VersionStatus::RepoBehind { binary, repo } => {
                let at = repo.as_deref().unwrap_or("an untagged commit");
                format!("hats is v{binary} but the repo is at {at}. Run `hats update`.")
            }
            VersionStatus::RepoAhead { binary, repo } => {
                format!("the repo is at {repo} but hats is v{binary}. Run `brew upgrade hats`.")
            }
            VersionStatus::Untagged { binary } => {
                format!("the repo is on an untagged commit; hats is v{binary}")
            }
            VersionStatus::Missing => "no dotfiles repo yet. Run `hats init`.".to_string(),
        }
    }
}

/// Compare a repo's current tag with this binary's version.
///
/// Pure, so every branch is testable: `current_tag` comes from git, everything
/// else is arithmetic on version strings.
pub fn classify(binary_version: &str, current_tag: Option<&str>, tags: &[String]) -> VersionStatus {
    let wanted = format!("v{binary_version}");
    match current_tag {
        Some(tag) if tag == wanted => VersionStatus::Match { tag: wanted },
        Some(tag) => {
            let repo_v = tag.strip_prefix('v').unwrap_or(tag);
            match (
                semver::Version::parse(repo_v),
                semver::Version::parse(binary_version),
            ) {
                (Ok(repo), Ok(bin)) if repo > bin => VersionStatus::RepoAhead {
                    binary: binary_version.to_string(),
                    repo: tag.to_string(),
                },
                _ => VersionStatus::RepoBehind {
                    binary: binary_version.to_string(),
                    repo: Some(tag.to_string()),
                },
            }
        }
        None => {
            // No tag on HEAD. If the wanted tag exists in the repo we are
            // simply not on it; otherwise this is a development clone.
            if tags.iter().any(|t| t == &wanted) {
                VersionStatus::RepoBehind {
                    binary: binary_version.to_string(),
                    repo: None,
                }
            } else {
                VersionStatus::Untagged {
                    binary: binary_version.to_string(),
                }
            }
        }
    }
}

/// The dotfiles clone.
pub struct Repo<'g> {
    pub path: PathBuf,
    git: &'g dyn Git,
}

impl<'g> Repo<'g> {
    pub fn new(path: impl Into<PathBuf>, git: &'g dyn Git) -> Self {
        Self {
            path: path.into(),
            git,
        }
    }

    pub fn exists(&self) -> bool {
        self.path.join(".git").exists()
    }

    fn require(&self) -> Result<(), RepoError> {
        if self.exists() {
            Ok(())
        } else {
            Err(RepoError::Missing {
                path: self.path.clone(),
            })
        }
    }

    pub fn clone_from(&self, url: &str) -> Result<(), RepoError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| RepoError::Spawn { source })?;
        }
        self.git.clone_repo(url, &self.path)
    }

    pub fn status(&self) -> Result<VersionStatus, RepoError> {
        if !self.exists() {
            return Ok(VersionStatus::Missing);
        }
        let tag = self.git.current_tag(&self.path)?;
        let tags = self.git.tags(&self.path)?;
        Ok(classify(BINARY_VERSION, tag.as_deref(), &tags))
    }

    pub fn is_dirty(&self) -> Result<bool, RepoError> {
        self.require()?;
        self.git.is_dirty(&self.path)
    }

    pub fn head_sha(&self) -> Result<String, RepoError> {
        self.require()?;
        self.git.head_sha(&self.path)
    }

    pub fn fetch_tags(&self) -> Result<(), RepoError> {
        self.require()?;
        self.git.fetch_tags(&self.path)
    }

    pub fn tags(&self) -> Result<Vec<String>, RepoError> {
        self.require()?;
        self.git.tags(&self.path)
    }

    /// Move the clone to the tag matching this binary.
    ///
    /// Refuses on a dirty tree unless forced: the clone is meant to be
    /// disposable, but silently discarding an edit someone made in there would
    /// be a nasty surprise.
    pub fn update_to_binary_version(&self, force: bool) -> Result<String, RepoError> {
        self.require()?;
        if !force && self.git.is_dirty(&self.path)? {
            return Err(RepoError::Dirty {
                path: self.path.clone(),
            });
        }
        self.git.fetch_tags(&self.path)?;
        let wanted = wanted_tag();
        if !self.git.tags(&self.path)?.iter().any(|t| t == &wanted) {
            return Err(RepoError::NoMatchingTag {
                binary: BINARY_VERSION.to_string(),
            });
        }
        self.git.checkout_detached(&self.path, &wanted)?;
        Ok(wanted)
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.path.join(crate::config::MANIFEST_NAME)
    }

    pub fn files_dir(&self) -> PathBuf {
        self.path.join(crate::config::FILES_DIR)
    }
}

impl std::fmt::Debug for Repo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repo").field("path", &self.path).finish()
    }
}

/// Convenience for the common case of the real git binary.
pub fn open(path: &Path) -> Repo<'static> {
    static GIT: git::GitCli = git::GitCli;
    Repo::new(path, &GIT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git::fake::FakeGit;

    fn tags(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn exact_tag_is_a_match() {
        let s = classify("0.4.0", Some("v0.4.0"), &tags(&["v0.4.0"]));
        assert_eq!(
            s,
            VersionStatus::Match {
                tag: "v0.4.0".into()
            }
        );
        assert!(s.is_ok());
        assert_eq!(s.exit_code(), 0);
    }

    #[test]
    fn an_older_repo_tag_means_run_update() {
        let s = classify("0.4.0", Some("v0.3.2"), &tags(&["v0.3.2", "v0.4.0"]));
        assert!(matches!(s, VersionStatus::RepoBehind { .. }));
        assert_eq!(s.exit_code(), 3);
        assert!(s.summary().contains("hats update"));
    }

    #[test]
    fn a_newer_repo_tag_means_upgrade_the_binary() {
        let s = classify("0.4.0", Some("v0.5.0"), &tags(&["v0.5.0"]));
        assert!(matches!(s, VersionStatus::RepoAhead { .. }));
        assert_eq!(s.exit_code(), 4);
        assert!(s.summary().contains("brew upgrade"));
    }

    #[test]
    fn untagged_head_with_the_wanted_tag_available_is_behind_not_untagged() {
        let s = classify("0.4.0", None, &tags(&["v0.4.0"]));
        assert_eq!(
            s,
            VersionStatus::RepoBehind {
                binary: "0.4.0".into(),
                repo: None
            }
        );
        assert!(s.summary().contains("untagged commit"));
    }

    #[test]
    fn a_development_clone_with_no_matching_tag_is_untagged() {
        let s = classify("0.4.0", None, &tags(&["v0.1.0"]));
        assert!(matches!(s, VersionStatus::Untagged { .. }));
    }

    #[test]
    fn an_unparseable_tag_is_treated_as_behind_rather_than_crashing() {
        let s = classify("0.4.0", Some("nightly"), &tags(&["nightly"]));
        assert!(matches!(s, VersionStatus::RepoBehind { .. }));
    }

    #[test]
    fn missing_clone_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let git = FakeGit::default();
        let repo = Repo::new(dir.path().join("absent"), &git);
        assert_eq!(repo.status().unwrap(), VersionStatus::Missing);
        assert_eq!(VersionStatus::Missing.exit_code(), 1);
    }

    #[test]
    fn update_refuses_a_dirty_tree_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let git = FakeGit {
            dirty: true,
            tags: tags(&[&wanted_tag()]),
            ..Default::default()
        };
        let repo = Repo::new(dir.path(), &git);

        let err = repo.update_to_binary_version(false).unwrap_err();
        assert!(matches!(err, RepoError::Dirty { .. }));
        assert!(git.checked_out.borrow().is_empty());

        repo.update_to_binary_version(true).unwrap();
        assert_eq!(*git.checked_out.borrow(), vec![wanted_tag()]);
    }

    #[test]
    fn update_explains_a_release_that_has_not_published_yet() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let git = FakeGit {
            tags: tags(&["v0.0.1"]),
            ..Default::default()
        };
        let repo = Repo::new(dir.path(), &git);
        let err = repo.update_to_binary_version(false).unwrap_err();
        assert!(matches!(err, RepoError::NoMatchingTag { .. }), "{err:?}");
        assert!(
            *git.fetched.borrow(),
            "must fetch before deciding the tag is absent"
        );
    }

    #[test]
    fn update_fetches_then_checks_out_the_wanted_tag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let git = FakeGit {
            tags: tags(&[&wanted_tag()]),
            ..Default::default()
        };
        let repo = Repo::new(dir.path(), &git);
        assert_eq!(repo.update_to_binary_version(false).unwrap(), wanted_tag());
        assert_eq!(*git.checked_out.borrow(), vec![wanted_tag()]);
    }

    #[test]
    fn operations_on_a_missing_clone_name_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let git = FakeGit::default();
        let repo = Repo::new(dir.path().join("absent"), &git);
        let err = repo.is_dirty().unwrap_err();
        assert!(matches!(err, RepoError::Missing { .. }));
        assert!(err.to_string().contains("hats init"));
    }

    #[test]
    fn wanted_tag_tracks_the_crate_version() {
        assert_eq!(wanted_tag(), format!("v{}", env!("CARGO_PKG_VERSION")));
    }
}
