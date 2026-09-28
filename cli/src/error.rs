//! Typed errors for the parts of hats where callers branch on the failure.
//!
//! Commands use `anyhow` for reporting; the engine and config layers return
//! these so tests can assert on the variant rather than on message text.

use std::path::PathBuf;

/// Something wrong with `~/.hats/config.yaml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no hat named `{name}` (known hats: {})", known.join(", "))]
    UnknownHat { name: String, known: Vec<String> },

    #[error("hat `{hat}` inherits from `{parent}`, which does not exist")]
    UnknownParent { hat: String, parent: String },

    #[error("hat `{hat}` inherits from itself: {chain}")]
    HatCycle { hat: String, chain: String },

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

    #[error("hats is not initialised on this machine. Run `hats init` first.")]
    NotInitialised,
}
