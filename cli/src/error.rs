//! Typed errors for the parts of hats where callers branch on the failure.
//!
//! Commands use `anyhow` for reporting; the engine and config layers return
//! these so tests can assert on the variant rather than on message text.

use std::path::PathBuf;

/// Something wrong with `hats.yaml` or `~/.hats/config.yaml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no profile named `{name}` (known profiles: {})", known.join(", "))]
    UnknownProfile { name: String, known: Vec<String> },

    #[error("profile `{profile}` inherits from `{parent}`, which does not exist")]
    UnknownParent { profile: String, parent: String },

    #[error("profile `{profile}` inherits from itself: {chain}")]
    ProfileCycle { profile: String, chain: String },

    #[error("{path} is not valid YAML")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml_ng::Error,
    },

    #[error("could not read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "{path} declares schema {found}, but this hats only understands up to {supported}. \
         Run `brew upgrade hats`."
    )]
    SchemaTooNew {
        path: PathBuf,
        found: u32,
        supported: u32,
    },

    #[error("hats is not initialised on this machine. Run `hats init` first.")]
    NotInitialised,
}

/// Something wrong with the dotfiles clone under `~/.hats/repo`.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("no dotfiles repo at {path}. Run `hats init` first.")]
    Missing { path: PathBuf },

    #[error("git {args} failed: {stderr}")]
    Git { args: String, stderr: String },

    #[error("could not run git")]
    Spawn {
        #[source]
        source: std::io::Error,
    },

    #[error(
        "the dotfiles repo has uncommitted changes at {path}. \
         Commit, stash, or re-run with --force."
    )]
    Dirty { path: PathBuf },

    #[error(
        "this hats is {binary}, but the repo has no tag v{binary}. \
         The release may still be publishing; try again shortly, or `hats update --check`."
    )]
    NoMatchingTag { binary: String },
}
