//! Configuration: one machine-local file, `~/.hats/config.yaml`.
//!
//! There is no repo manifest any more: the dotfiles engine that needed one is
//! `bosun`, a sibling tool. Everything hats reads lives on this machine.

pub mod hat;
pub mod local;

use std::collections::BTreeSet;

use indexmap::IndexMap;

use crate::error::ConfigError;
use crate::paths::HatsPaths;
use hat::{HatSpec, ResolvedHat};
use local::LocalConfig;

/// The machine configuration every command sees.
#[derive(Debug, Clone)]
pub struct Config {
    pub local: LocalConfig,
}

impl Config {
    pub fn load(paths: &HatsPaths) -> Result<Self, ConfigError> {
        let local = LocalConfig::load(&paths.config)?;
        Ok(Self { local })
    }

    pub fn hats(&self) -> &IndexMap<String, HatSpec> {
        &self.local.hats
    }

    pub fn hat_names(&self) -> Vec<String> {
        self.local.hats.keys().cloned().collect()
    }

    /// Fold a hat's inheritance chain, filling gaps from the machine
    /// identity.
    pub fn resolve_hat(&self, name: &str) -> Result<ResolvedHat, ConfigError> {
        hat::resolve(name, &self.local.hats, self.local.identity.as_ref())
    }

    /// The unset list emitted before every hat switch: the union of every
    /// hat's variables, so the previous hat cannot leak into the next.
    pub fn all_env_keys(&self) -> BTreeSet<String> {
        hat::all_env_keys(&self.local.hats, self.local.identity.as_ref())
    }

    /// Per-hat ssh files: scaffolds under ~/.ssh/config.d and the
    /// CODER_SSH_CONFIG_FILE isolation.
    pub fn ssh_enabled(&self) -> bool {
        self.local.features.ssh
    }

    /// Per-hat VS Code profiles and the `code()` shell wrapper.
    pub fn vscode_enabled(&self) -> bool {
        self.local.features.vscode
    }

    /// Every problem `hats doctor` should report about the configuration.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();

        for name in self.local.hats.keys() {
            if let Err(e) = self.resolve_hat(name) {
                problems.push(e.to_string());
            }
        }

        if let Some(default) = &self.local.meta.default_hat
            && !self.local.hats.contains_key(default)
        {
            problems.push(format!(
                "default_hat is `{default}`, which is not a declared hat"
            ));
        }

        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(local_yaml: &str) -> Config {
        Config {
            local: serde_yaml_ng::from_str(local_yaml).unwrap(),
        }
    }

    #[test]
    fn features_default_to_ssh_on_and_vscode_off() {
        let c = config("hats:\n  normal: {}\n");
        assert!(c.ssh_enabled());
        assert!(!c.vscode_enabled());
    }

    #[test]
    fn features_can_be_turned_by_the_config() {
        let c = config("features: { ssh: false, vscode: true }\nhats:\n  normal: {}\n");
        assert!(!c.ssh_enabled());
        assert!(c.vscode_enabled());
    }

    #[test]
    fn problems_report_a_bad_default_hat() {
        let c = config("meta: { default_hat: ghost }\nhats:\n  normal: {}\n");
        let problems = c.problems();
        assert!(problems.iter().any(|p| p.contains("ghost")), "{problems:?}");
    }

    #[test]
    fn problems_report_a_profile_cycle() {
        let c = config("hats:\n  a: { inherits: b }\n  b: { inherits: a }\n");
        assert!(
            c.problems()
                .iter()
                .any(|p| p.contains("inherits from itself"))
        );
    }

    #[test]
    fn a_clean_config_has_no_problems() {
        let c = config("hats:\n  normal: {}\n  work: { inherits: normal }\n");
        assert_eq!(c.problems(), Vec::<String>::new());
    }

    #[test]
    fn resolve_and_env_keys_reach_through_the_config() {
        let c = config(
            "identity: { name: Jane, email: r@e.com }\nhats:\n  normal: {}\n  work:\n    inherits: normal\n    env: { TOK: x }\n",
        );
        let work = c.resolve_hat("work").unwrap();
        assert_eq!(work.identity.name.as_deref(), Some("Jane"));
        assert!(c.all_env_keys().contains("TOK"));
        assert_eq!(c.hat_names(), vec!["normal", "work"]);
    }

    #[test]
    fn stale_keys_from_before_the_bosun_split_still_parse() {
        // groups: and meta.repo used to exist; serde ignores them rather than
        // failing the whole config.
        let c = config(
            "meta: { repo: 'https://example.com/x.git' }\ngroups: { shell: true }\nhats:\n  normal: {}\n",
        );
        assert_eq!(c.hat_names(), vec!["normal"]);
    }
}
