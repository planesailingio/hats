//! Configuration: the repo manifest plus the machine-local file, merged.
//!
//! Two files, one type. [`Config`] is what every command sees, so no command
//! has to know which half a value came from.

pub mod condition;
pub mod local;
pub mod hat;
pub mod repo;

use std::collections::BTreeSet;

use indexmap::IndexMap;

use crate::error::ConfigError;
use crate::paths::HatsPaths;
use local::LocalConfig;
use hat::{HatSpec, ResolvedHat};
use repo::RepoConfig;

/// The manifest file inside the dotfiles repo.
pub const MANIFEST_NAME: &str = "hats.yaml";
/// Directory inside the repo holding the managed files.
pub const FILES_DIR: &str = "files";

/// Repo manifest and machine config, loaded together.
#[derive(Debug, Clone)]
pub struct Config {
    pub repo: RepoConfig,
    pub local: LocalConfig,
}

impl Config {
    pub fn load(paths: &HatsPaths) -> Result<Self, ConfigError> {
        let local = LocalConfig::load(&paths.config)?;
        let repo = RepoConfig::load(&paths.repo.join(MANIFEST_NAME))?;
        Ok(Self { repo, local })
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

    /// Groups enabled on this machine, honouring the manifest default for any
    /// group the wizard has not been asked about yet (a group added upstream
    /// after `hats init` ran).
    pub fn group_enabled(&self, group: &str) -> bool {
        match self.local.groups.get(group) {
            Some(answer) => *answer,
            None => self.repo.groups.get(group).is_some_and(|g| g.default),
        }
    }

    /// Every problem `hats lint` should report about the configuration pair.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = self.repo.group_problems();

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

        // Secret references that the repo does not expect. Advisory only.
        if !self.repo.secrets.required.is_empty() {
            let known: BTreeSet<&str> = self
                .repo
                .secrets
                .required
                .iter()
                .map(String::as_str)
                .collect();
            for name in self.local.hats.keys() {
                if let Ok(p) = self.resolve_hat(name) {
                    for r in p.secret_refs() {
                        if !known.contains(r.as_str()) {
                            problems.push(format!(
                                "hat `{name}` refers to secret `{r}`, which is not in the repo's secrets.required list"
                            ));
                        }
                    }
                }
            }
        }

        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::repo::GroupSpec;

    fn config(repo_yaml: &str, local_yaml: &str) -> Config {
        Config {
            repo: serde_yaml_ng::from_str(repo_yaml).unwrap(),
            local: serde_yaml_ng::from_str(local_yaml).unwrap(),
        }
    }

    #[test]
    fn an_unanswered_group_uses_the_manifest_default() {
        let c = config(
            "groups:\n  shell: { description: s, default: true }\n  extra: { description: e, default: false }\n",
            "groups: {}\n",
        );
        assert!(c.group_enabled("shell"));
        assert!(!c.group_enabled("extra"));
    }

    #[test]
    fn the_machine_answer_overrides_the_manifest_default() {
        let c = config(
            "groups:\n  shell: { description: s, default: true }\n",
            "groups: { shell: false }\n",
        );
        assert!(!c.group_enabled("shell"));
    }

    #[test]
    fn an_unknown_group_is_disabled_rather_than_assumed() {
        let c = config("groups: {}\n", "groups: {}\n");
        assert!(!c.group_enabled("ghost"));
    }

    #[test]
    fn problems_report_a_bad_default_hat() {
        let c = config(
            "groups: {}\n",
            "meta: { default_hat: ghost }\nhats:\n  normal: {}\n",
        );
        let problems = c.problems();
        assert!(problems.iter().any(|p| p.contains("ghost")), "{problems:?}");
    }

    #[test]
    fn problems_report_a_profile_cycle() {
        let c = config(
            "groups: {}\n",
            "hats:\n  a: { inherits: b }\n  b: { inherits: a }\n",
        );
        assert!(
            c.problems()
                .iter()
                .any(|p| p.contains("inherits from itself"))
        );
    }

    #[test]
    fn problems_flag_a_secret_the_repo_does_not_expect() {
        let c = config(
            "groups: {}\nsecrets:\n  required: [git_signing_key]\n",
            "hats:\n  normal:\n    env:\n      TOK: { secret: mystery }\n",
        );
        let problems = c.problems();
        assert!(
            problems.iter().any(|p| p.contains("mystery")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_clean_pair_has_no_problems() {
        let c = config(
            "groups:\n  shell: { description: s }\nfiles:\n  - { path: .zshrc.j2, group: shell }\n",
            "hats:\n  normal: {}\n  work: { inherits: normal }\n",
        );
        assert_eq!(c.problems(), Vec::<String>::new());
    }

    #[test]
    fn resolve_and_env_keys_reach_through_the_merged_config() {
        let c = config(
            "groups: {}\n",
            "identity: { name: Jane, email: r@e.com }\nhats:\n  normal: {}\n  work:\n    inherits: normal\n    env: { TOK: x }\n",
        );
        let work = c.resolve_hat("work").unwrap();
        assert_eq!(work.identity.name.as_deref(), Some("Jane"));
        assert!(c.all_env_keys().contains("TOK"));
        assert_eq!(c.hat_names(), vec!["normal", "work"]);
    }

    #[test]
    fn group_spec_default_defaults_to_true() {
        let g: GroupSpec = serde_yaml_ng::from_str("description: d\n").unwrap();
        assert!(g.default);
    }
}
