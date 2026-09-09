//! `~/.hats/state.yaml` — what the last apply actually did.
//!
//! State exists for two things the plan cannot work out from the manifest
//! alone:
//!
//! 1. **Destroy detection.** A file managed last time but not this time is an
//!    orphan. Without state, disabling a group would silently leave its files
//!    behind for ever.
//! 2. **Safe pruning.** The recorded hash says whether the user has edited the
//!    file since hats wrote it. An untouched orphan can be removed; an edited
//!    one is reported and left alone.
//!
//! Hook markers live here too: `once` hooks record that they ran, `onchange`
//! hooks record the hash of their declared inputs.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Bumped when the on-disk shape changes incompatibly.
pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    /// SHA-256 of the content hats last wrote.
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookState {
    /// Hash of the hook's declared inputs, for `onchange` triggers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    pub ran_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    /// Keyed by absolute target path.
    #[serde(default)]
    pub files: BTreeMap<String, FileState>,
    /// Keyed by hook name.
    #[serde(default)]
    pub hooks: BTreeMap<String, HookState>,
}

fn default_version() -> u32 {
    STATE_VERSION
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            applied_at: None,
            files: BTreeMap::new(),
            hooks: BTreeMap::new(),
        }
    }
}

impl State {
    /// Load, or start empty. A missing state file is the normal first-run case
    /// and must never be an error.
    ///
    /// A state file from a *newer* hats is also treated as empty rather than
    /// refused: the worst outcome is that orphans are not detected, which is
    /// safe, whereas refusing would block the apply that fixes the skew.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let state: Self = serde_yaml_ng::from_str(&text)
            .with_context(|| format!("parsing {}", path.display()))?;
        if state.version > STATE_VERSION {
            return Ok(Self::default());
        }
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let body = serde_yaml_ng::to_string(self)?;
        let contents = format!(
            "# ~/.hats/state.yaml — what the last `hats apply` wrote.\n\
             # Used to detect files that are no longer managed. Do not edit.\n{body}"
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("yaml.tmp");
        std::fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }

    pub fn record_file(&mut self, target: &Path, contents: &[u8], mode: Option<u32>) {
        self.files.insert(
            target.to_string_lossy().into_owned(),
            FileState {
                hash: hash(contents),
                mode,
            },
        );
    }

    pub fn forget_file(&mut self, target: &Path) {
        self.files.remove(&target.to_string_lossy().into_owned());
    }

    pub fn file(&self, target: &Path) -> Option<&FileState> {
        self.files.get(&target.to_string_lossy().into_owned())
    }

    pub fn record_hook(&mut self, name: &str, hash: Option<String>) {
        self.hooks.insert(
            name.to_string(),
            HookState {
                hash,
                ran_at: chrono::Utc::now().to_rfc3339(),
            },
        );
    }

    pub fn hook(&self, name: &str) -> Option<&HookState> {
        self.hooks.get(name)
    }

    pub fn touch(&mut self) {
        self.applied_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Targets recorded last time that are not in `current`: the destroy
    /// candidates.
    pub fn orphans(&self, current: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
        let live: std::collections::BTreeSet<String> = current
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        self.files
            .keys()
            .filter(|k| !live.contains(*k))
            .map(std::path::PathBuf::from)
            .collect()
    }
}

/// SHA-256, hex encoded. Used for file content and hook inputs alike.
pub fn hash(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Hash a set of files together, in the given order. Missing files contribute
/// a marker rather than being skipped, so deleting an input still counts as a
/// change.
pub fn hash_paths(paths: &[std::path::PathBuf]) -> String {
    let mut h = Sha256::new();
    for p in paths {
        h.update(p.to_string_lossy().as_bytes());
        match std::fs::read(p) {
            Ok(bytes) => h.update(&bytes),
            Err(_) => h.update(b"<missing>"),
        }
    }
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn round_trips_through_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.yaml");

        let mut s = State::default();
        s.record_file(Path::new("/home/t/.zshrc"), b"contents", Some(0o644));
        s.record_hook("brew-bundle", Some("abc123".into()));
        s.touch();
        s.save(&path).unwrap();

        let back = State::load(&path).unwrap();
        assert_eq!(back.version, STATE_VERSION);
        assert!(back.applied_at.is_some());
        assert_eq!(
            back.file(Path::new("/home/t/.zshrc")).unwrap().hash,
            hash(b"contents")
        );
        assert_eq!(
            back.hook("brew-bundle").unwrap().hash.as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn a_missing_state_file_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let s = State::load(&dir.path().join("absent.yaml")).unwrap();
        assert!(s.files.is_empty());
        assert!(s.hooks.is_empty());
    }

    /// Refusing a newer state file would block the very apply that fixes a
    /// version skew, so it is ignored instead.
    #[test]
    fn a_newer_state_file_is_ignored_rather_than_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.yaml");
        std::fs::write(&path, "version: 999\nfiles:\n  /x: { hash: abc }\n").unwrap();
        let s = State::load(&path).unwrap();
        assert!(s.files.is_empty());
    }

    #[test]
    fn orphans_are_targets_no_longer_managed() {
        let mut s = State::default();
        s.record_file(Path::new("/home/t/.zshrc"), b"a", None);
        s.record_file(Path::new("/home/t/.env.d/dev.zsh"), b"b", None);

        let orphans = s.orphans(&[PathBuf::from("/home/t/.zshrc")]);
        assert_eq!(orphans, vec![PathBuf::from("/home/t/.env.d/dev.zsh")]);

        assert!(
            s.orphans(&[
                PathBuf::from("/home/t/.zshrc"),
                PathBuf::from("/home/t/.env.d/dev.zsh")
            ])
            .is_empty()
        );
    }

    #[test]
    fn forgetting_a_file_removes_it_from_the_next_orphan_check() {
        let mut s = State::default();
        s.record_file(Path::new("/home/t/.gone"), b"x", None);
        s.forget_file(Path::new("/home/t/.gone"));
        assert!(s.orphans(&[]).is_empty());
    }

    #[test]
    fn hashing_is_stable_and_content_sensitive() {
        assert_eq!(hash(b"same"), hash(b"same"));
        assert_ne!(hash(b"a"), hash(b"b"));
        assert_eq!(hash(b"").len(), 64);
    }

    #[test]
    fn hashing_a_set_of_paths_notices_content_and_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("Brewfile");
        let b = dir.path().join("hook.sh");
        std::fs::write(&a, "brew 'jq'\n").unwrap();
        std::fs::write(&b, "echo hi\n").unwrap();
        let paths = vec![a.clone(), b.clone()];

        let before = hash_paths(&paths);
        assert_eq!(before, hash_paths(&paths), "stable for unchanged inputs");

        std::fs::write(&a, "brew 'jq'\nbrew 'fd'\n").unwrap();
        let after_edit = hash_paths(&paths);
        assert_ne!(before, after_edit, "an edited input must change the hash");

        std::fs::remove_file(&b).unwrap();
        assert_ne!(after_edit, hash_paths(&paths), "a deleted input must too");
    }

    #[test]
    fn path_order_is_part_of_the_hash() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, "1").unwrap();
        std::fs::write(&b, "2").unwrap();
        assert_ne!(
            hash_paths(&[a.clone(), b.clone()]),
            hash_paths(&[b, a]),
            "reordering declared inputs is a change"
        );
    }
}
