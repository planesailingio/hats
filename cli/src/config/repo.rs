//! `hats.yaml` — the manifest that ships in the dotfiles repo.
//!
//! Deliberately generic: it declares which files exist, how they are grouped
//! for the init wizard, and which hooks run. It contains no client names, no
//! identities and no secret item names — all of that is machine-local, in
//! [`super::local`].

use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::config::condition::Condition;
use crate::error::ConfigError;

/// Highest `hats.schema` this binary understands. A repo declaring more is
/// refused with an instruction to upgrade, which matters because the repo and
/// the binary are released from one tag and can only skew during an upgrade.
pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    #[serde(default)]
    pub hats: RepoMeta,
    /// Groups the init wizard asks about, one yes/no each.
    #[serde(default)]
    pub groups: IndexMap<String, GroupSpec>,
    #[serde(default)]
    pub files: Vec<FileSpec>,
    #[serde(default)]
    pub hooks: Vec<HookSpec>,
    #[serde(default)]
    pub secrets: RepoSecrets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMeta {
    #[serde(default = "default_schema")]
    pub schema: u32,
}

impl Default for RepoMeta {
    fn default() -> Self {
        Self {
            schema: default_schema(),
        }
    }
}

fn default_schema() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoSecrets {
    /// Secret keys the templates and hats are expected to use. `hats lint`
    /// warns about references outside this list; it is advisory, never a gate,
    /// because the machine config is free to add its own.
    #[serde(default)]
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSpec {
    pub description: String,
    /// The answer offered in the wizard when the user just presses enter.
    #[serde(default = "default_true")]
    pub default: bool,
}

fn default_true() -> bool {
    true
}

/// One managed path. `path` is relative to `files/` in the repo; the target is
/// `~/` plus that path with a trailing `.j2` removed. Directories are managed
/// recursively.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSpec {
    pub path: PathBuf,
    pub group: String,
    /// Octal mode for the rendered file, e.g. "0600".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Octal mode for directories created along the way, e.g. "0700".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir_mode: Option<String>,
    /// Rendered content contains secrets, so its diff is withheld unless
    /// `--show-secrets`.
    #[serde(default)]
    pub secret: bool,
    #[serde(default, flatten, skip_serializing_if = "Condition::is_empty")]
    pub condition: Condition,
}

impl FileSpec {
    /// A `.j2` suffix marks a template; everything else is copied verbatim.
    pub fn is_template(&self) -> bool {
        self.path.extension().is_some_and(|e| e == "j2")
    }

    /// Path under `$HOME` this renders to.
    pub fn target_rel(&self) -> PathBuf {
        match self.path.to_str() {
            Some(s) => PathBuf::from(s.strip_suffix(".j2").unwrap_or(s)),
            None => self.path.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Runs before any file is written (e.g. installing Homebrew).
    Before,
    /// Runs after the files are in place (the common case).
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SimpleTrigger {
    /// Run the first time only, then never again unless forced.
    Once,
    /// Run on every apply.
    Always,
}

/// When a hook is due. `onchange` replaces chezmoi's trick of embedding a
/// `sha256sum` comment in the script: the inputs are declared, so the hash is
/// computed over exactly the files that matter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Trigger {
    Simple(SimpleTrigger),
    OnChange { onchange: Vec<PathBuf> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookSpec {
    pub name: String,
    pub phase: Phase,
    pub trigger: Trigger,
    /// Script path relative to the repo root.
    pub script: PathBuf,
    #[serde(default, flatten, skip_serializing_if = "Condition::is_empty")]
    pub condition: Condition,
}

impl RepoConfig {
    pub fn load(path: &std::path::Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let cfg: RepoConfig =
            serde_yaml_ng::from_str(&text).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        if cfg.hats.schema > SUPPORTED_SCHEMA {
            return Err(ConfigError::SchemaTooNew {
                path: path.to_path_buf(),
                found: cfg.hats.schema,
                supported: SUPPORTED_SCHEMA,
            });
        }
        Ok(cfg)
    }

    /// Groups referenced by files but never declared, and vice versa. Returned
    /// rather than logged so `hats lint` owns the presentation.
    pub fn group_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for f in &self.files {
            if !self.groups.contains_key(&f.group) {
                problems.push(format!(
                    "file `{}` is in group `{}`, which is not declared under groups:",
                    f.path.display(),
                    f.group
                ));
            }
        }
        for name in self.groups.keys() {
            if !self.files.iter().any(|f| &f.group == name) {
                problems.push(format!("group `{name}` has no files"));
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
hats:
  schema: 1
groups:
  shell: { description: "zsh", default: true }
  ssh:   { description: "ssh config", default: true }
files:
  - { path: .zshrc.j2, group: shell }
  - { path: .ssh/config.j2, group: ssh, mode: "0600", dir_mode: "0700" }
  - { path: .config/bat, group: shell }
hooks:
  - { name: brew-bundle, phase: after, trigger: { onchange: [Brewfile] }, script: hooks/brew-bundle.sh }
  - { name: dock, phase: after, trigger: once, script: hooks/dock.sh, os: [darwin] }
"#;

    fn parse() -> RepoConfig {
        serde_yaml_ng::from_str(SAMPLE).unwrap()
    }

    #[test]
    fn parses_the_manifest() {
        let c = parse();
        assert_eq!(c.hats.schema, 1);
        assert_eq!(c.files.len(), 3);
        assert_eq!(c.hooks.len(), 2);
    }

    #[test]
    fn j2_marks_a_template_and_is_stripped_from_the_target() {
        let c = parse();
        assert!(c.files[0].is_template());
        assert_eq!(c.files[0].target_rel(), PathBuf::from(".zshrc"));
        // A plain directory is copied, not rendered.
        assert!(!c.files[2].is_template());
        assert_eq!(c.files[2].target_rel(), PathBuf::from(".config/bat"));
    }

    #[test]
    fn modes_survive_the_round_trip() {
        let c = parse();
        assert_eq!(c.files[1].mode.as_deref(), Some("0600"));
        assert_eq!(c.files[1].dir_mode.as_deref(), Some("0700"));
    }

    #[test]
    fn triggers_parse_in_both_shapes() {
        let c = parse();
        assert_eq!(
            c.hooks[0].trigger,
            Trigger::OnChange {
                onchange: vec![PathBuf::from("Brewfile")]
            }
        );
        assert_eq!(c.hooks[1].trigger, Trigger::Simple(SimpleTrigger::Once));
    }

    #[test]
    fn a_hook_condition_is_flattened_alongside_its_other_fields() {
        let c = parse();
        assert_eq!(c.hooks[1].condition.os, vec![crate::platform::Os::Darwin]);
        assert!(c.hooks[0].condition.is_empty());
    }

    #[test]
    fn a_newer_schema_is_refused_with_an_upgrade_hint() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hats.yaml");
        std::fs::write(&p, "hats:\n  schema: 99\n").unwrap();
        let err = RepoConfig::load(&p).unwrap_err();
        assert!(
            matches!(err, ConfigError::SchemaTooNew { found: 99, .. }),
            "{err:?}"
        );
        assert!(err.to_string().contains("brew upgrade hats"));
    }

    #[test]
    fn group_problems_catch_both_directions() {
        let c: RepoConfig = serde_yaml_ng::from_str(
            "groups:\n  used: { description: u }\n  orphan: { description: o }\nfiles:\n  - { path: a, group: used }\n  - { path: b, group: ghost }\n",
        )
        .unwrap();
        let problems = c.group_problems();
        assert!(problems.iter().any(|p| p.contains("ghost")));
        assert!(problems.iter().any(|p| p.contains("orphan")));
    }
}
