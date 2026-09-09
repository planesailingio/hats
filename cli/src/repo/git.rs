//! Git, behind a trait.
//!
//! hats shells out to `git` rather than linking libgit2. The operations it
//! needs are trivial (clone, fetch, tag list, checkout, status), git is always
//! present on a machine that has a dotfiles repo, and staying off libgit2 keeps
//! the release cross-compiles free of OpenSSL and libssh2. The trait exists so
//! version logic can be tested without a real repository.

use std::path::Path;
use std::process::Command;

use crate::error::RepoError;

pub trait Git {
    fn clone_repo(&self, url: &str, dest: &Path) -> Result<(), RepoError>;
    fn fetch_tags(&self, repo: &Path) -> Result<(), RepoError>;
    fn tags(&self, repo: &Path) -> Result<Vec<String>, RepoError>;
    fn checkout_detached(&self, repo: &Path, refname: &str) -> Result<(), RepoError>;
    /// The tag pointing exactly at HEAD, if any.
    fn current_tag(&self, repo: &Path) -> Result<Option<String>, RepoError>;
    fn is_dirty(&self, repo: &Path) -> Result<bool, RepoError>;
    fn head_sha(&self, repo: &Path) -> Result<String, RepoError>;
}

/// The real thing.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitCli;

impl GitCli {
    fn run(&self, dir: Option<&Path>, args: &[&str]) -> Result<String, RepoError> {
        let mut cmd = Command::new("git");
        if let Some(d) = dir {
            cmd.arg("-C").arg(d);
        }
        cmd.args(args);
        // Never let git open an editor or a credential prompt mid-command.
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        let out = cmd.output().map_err(|source| RepoError::Spawn { source })?;
        if !out.status.success() {
            return Err(RepoError::Git {
                args: args.join(" "),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

impl Git for GitCli {
    fn clone_repo(&self, url: &str, dest: &Path) -> Result<(), RepoError> {
        let dest = dest.to_string_lossy().to_string();
        self.run(None, &["clone", url, &dest])?;
        Ok(())
    }

    fn fetch_tags(&self, repo: &Path) -> Result<(), RepoError> {
        self.run(Some(repo), &["fetch", "--tags", "--prune", "--quiet"])?;
        Ok(())
    }

    fn tags(&self, repo: &Path) -> Result<Vec<String>, RepoError> {
        let out = self.run(Some(repo), &["tag", "--list"])?;
        Ok(out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    fn checkout_detached(&self, repo: &Path, refname: &str) -> Result<(), RepoError> {
        self.run(Some(repo), &["checkout", "--detach", "--quiet", refname])?;
        Ok(())
    }

    fn current_tag(&self, repo: &Path) -> Result<Option<String>, RepoError> {
        match self.run(Some(repo), &["describe", "--tags", "--exact-match", "HEAD"]) {
            Ok(tag) if !tag.is_empty() => Ok(Some(tag)),
            // No tag on HEAD is a normal state, not a failure.
            Ok(_) | Err(RepoError::Git { .. }) => Ok(None),
            Err(other) => Err(other),
        }
    }

    fn is_dirty(&self, repo: &Path) -> Result<bool, RepoError> {
        Ok(!self.run(Some(repo), &["status", "--porcelain"])?.is_empty())
    }

    fn head_sha(&self, repo: &Path) -> Result<String, RepoError> {
        self.run(Some(repo), &["rev-parse", "--short", "HEAD"])
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::cell::RefCell;

    /// An in-memory git for testing version logic.
    #[derive(Debug, Default)]
    pub struct FakeGit {
        pub tags: Vec<String>,
        pub current_tag: Option<String>,
        pub dirty: bool,
        pub checked_out: RefCell<Vec<String>>,
        pub fetched: RefCell<bool>,
    }

    impl Git for FakeGit {
        fn clone_repo(&self, _url: &str, _dest: &Path) -> Result<(), RepoError> {
            Ok(())
        }
        fn fetch_tags(&self, _repo: &Path) -> Result<(), RepoError> {
            *self.fetched.borrow_mut() = true;
            Ok(())
        }
        fn tags(&self, _repo: &Path) -> Result<Vec<String>, RepoError> {
            Ok(self.tags.clone())
        }
        fn checkout_detached(&self, _repo: &Path, refname: &str) -> Result<(), RepoError> {
            self.checked_out.borrow_mut().push(refname.to_string());
            Ok(())
        }
        fn current_tag(&self, _repo: &Path) -> Result<Option<String>, RepoError> {
            Ok(self.current_tag.clone())
        }
        fn is_dirty(&self, _repo: &Path) -> Result<bool, RepoError> {
            Ok(self.dirty)
        }
        fn head_sha(&self, _repo: &Path) -> Result<String, RepoError> {
            Ok("deadbee".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the real CLI wrapper against a throwaway repository, so the
    /// argument strings are known to be right rather than assumed.
    #[test]
    fn drives_a_real_repository() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let git = GitCli;

        for args in [
            vec!["init", "--quiet", "-b", "main"],
            vec!["config", "user.email", "t@example.com"],
            vec!["config", "user.name", "T"],
            vec!["commit", "--quiet", "--allow-empty", "-m", "one"],
        ] {
            git.run(Some(repo), &args).unwrap();
        }

        assert!(!git.is_dirty(repo).unwrap());
        assert_eq!(git.current_tag(repo).unwrap(), None);
        assert!(git.tags(repo).unwrap().is_empty());

        git.run(Some(repo), &["tag", "-a", "v0.1.0", "-m", "x"])
            .unwrap();
        assert_eq!(git.tags(repo).unwrap(), vec!["v0.1.0"]);
        assert_eq!(git.current_tag(repo).unwrap().as_deref(), Some("v0.1.0"));

        std::fs::write(repo.join("new"), "x").unwrap();
        assert!(git.is_dirty(repo).unwrap());
        assert_eq!(git.head_sha(repo).unwrap().len(), 7);
    }

    #[test]
    fn a_failing_command_reports_the_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let err = GitCli.tags(dir.path()).unwrap_err();
        match err {
            RepoError::Git { args, .. } => assert_eq!(args, "tag --list"),
            other => panic!("expected a Git error, got {other:?}"),
        }
    }
}
