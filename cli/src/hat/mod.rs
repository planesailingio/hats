//! Turning a hat into a shell environment.
//!
//! A child process cannot change its parent's environment, so `hats env <name>`
//! prints shell code and the shell integration `eval`s it. Everything the
//! switch does is decided here, in one place, so `hats env`, the tests and the
//! documentation cannot disagree.
//!
//! Rule 3 of the design ("unset before you set") is enforced structurally: the
//! unset list is the union of every hat's keys, not a list anyone maintains.

pub mod aws;
pub mod emit;
pub mod kube;

use std::path::PathBuf;

use indexmap::IndexMap;

use crate::config::Config;
use crate::config::hat::{EnvValue, ResolvedHat};
use crate::error::ConfigError;
use crate::secrets::store::Secrets;

/// Everything a hat switch changes, resolved and ready to emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvPlan {
    pub hat: String,
    /// Cleared first, always. Sorted for a stable, snapshot-testable output.
    pub unset: Vec<String>,
    /// Exported in insertion order, which matters because later values may
    /// refer to earlier ones.
    pub set: IndexMap<String, String>,
    /// Prepended to PATH, guarded against duplication.
    pub path_prepend: Vec<String>,
    pub kubeconfig: Option<PathBuf>,
    pub kube_context: Option<String>,
    /// This hat's own AWS files, or None when isolation is off.
    pub aws_config: Option<PathBuf>,
    pub aws_credentials: Option<PathBuf>,
    /// Terminal tint, or None to leave the background alone.
    pub colour: Option<String>,
    /// Secrets the hat refers to that have no value. Reported to stderr so
    /// a shell never silently gets an empty token.
    pub missing_secrets: Vec<String>,
}

/// Options that let a caller emit a partial switch.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnvOptions {
    /// Skip the kubeconfig isolation and context selection.
    pub no_kube: bool,
    /// Skip the AWS config and credentials isolation.
    pub no_aws: bool,
    /// Skip the terminal background escape.
    pub no_colour: bool,
    /// Emit only the unset block.
    pub reset_only: bool,
}

impl EnvPlan {
    /// Build the plan for `name`.
    ///
    /// `home` is passed rather than read so tests can run against a temp
    /// directory without touching the developer's real `~/.kube`.
    pub fn build(
        cfg: &Config,
        name: &str,
        secrets: &Secrets,
        home: &std::path::Path,
        opts: EnvOptions,
    ) -> Result<Self, ConfigError> {
        let p = cfg.resolve_hat(name)?;

        let mut plan = Self {
            hat: name.to_string(),
            // The union across every hat. This is the leak fix.
            unset: cfg.all_env_keys().into_iter().collect(),
            set: IndexMap::new(),
            path_prepend: Vec::new(),
            kubeconfig: None,
            kube_context: None,
            aws_config: None,
            aws_credentials: None,
            colour: None,
            missing_secrets: Vec::new(),
        };

        if opts.reset_only {
            return Ok(plan);
        }

        plan.add_identity(&p, secrets);
        plan.add_env(&p, secrets);
        plan.path_prepend = p.path.iter().map(|s| expand_home(s, home)).collect();

        if !opts.no_aws && p.aws_isolated() {
            plan.aws_config = Some(aws::config_path(home, name));
            plan.aws_credentials = Some(aws::credentials_path(home, name));
        }
        if let Some(f) = &plan.aws_config {
            plan.set
                .insert("AWS_CONFIG_FILE".into(), f.to_string_lossy().into_owned());
        }
        if let Some(f) = &plan.aws_credentials {
            plan.set.insert(
                "AWS_SHARED_CREDENTIALS_FILE".into(),
                f.to_string_lossy().into_owned(),
            );
        }

        if !opts.no_kube && p.kube_isolated() {
            plan.kubeconfig = Some(kube::config_path(home, name));
            plan.kube_context = p.kube.context.clone();
        }
        if let Some(kc) = &plan.kubeconfig {
            plan.set
                .insert("KUBECONFIG".into(), kc.to_string_lossy().into_owned());
        }

        if !opts.no_colour {
            plan.colour = p.colour.clone();
        }

        // Set last, deliberately. The old zsh had a trap where a hat that
        plan.set.insert("HATS_HAT".into(), name.to_string());

        plan.missing_secrets = secrets.missing(p.secret_refs().iter().map(String::as_str));
        Ok(plan)
    }

    fn add_identity(&mut self, p: &ResolvedHat, secrets: &Secrets) {
        if let Some(n) = &p.identity.name {
            self.set.insert("GIT_AUTHOR_NAME".into(), n.clone());
            self.set.insert("GIT_COMMITTER_NAME".into(), n.clone());
        }
        if let Some(e) = &p.identity.email {
            self.set.insert("GIT_AUTHOR_EMAIL".into(), e.clone());
            self.set.insert("GIT_COMMITTER_EMAIL".into(), e.clone());
        }
        // git 2.31+ reads GIT_CONFIG_COUNT/KEY_n/VALUE_n, which is how any git
        // setting becomes per-shell without touching ~/.gitconfig.
        if let Some(key) = &p.identity.signing_key {
            let value = resolve(key, secrets);
            if !value.is_empty() {
                self.set.insert("GIT_CONFIG_COUNT".into(), "1".into());
                self.set
                    .insert("GIT_CONFIG_KEY_0".into(), "user.signingkey".into());
                self.set.insert("GIT_CONFIG_VALUE_0".into(), value);
            }
        }
    }

    fn add_env(&mut self, p: &ResolvedHat, secrets: &Secrets) {
        for (k, v) in &p.env {
            self.set.insert(k.clone(), resolve(v, secrets));
        }
    }

    /// Keys this plan actually exports, for tests and `hats hat show`.
    pub fn exported(&self) -> Vec<&str> {
        self.set.keys().map(String::as_str).collect()
    }
}

fn resolve(v: &EnvValue, secrets: &Secrets) -> String {
    match v {
        EnvValue::Literal(s) => s.clone(),
        EnvValue::Secret { secret } => secrets.value_or_empty(secret),
    }
}

/// Expand a leading `~` so paths in the config read naturally.
fn expand_home(path: &str, home: &std::path::Path) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest).to_string_lossy().into_owned(),
        None if path == "~" => home.to_string_lossy().into_owned(),
        None => path.to_string(),
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use crate::config::local::LocalConfig;

    pub fn config(local_yaml: &str) -> Config {
        Config {
            repo: serde_yaml_ng::from_str("groups: {}\n").unwrap(),
            local: serde_yaml_ng::from_str::<LocalConfig>(local_yaml).unwrap(),
        }
    }

    pub fn secrets(pairs: &[(&str, &str)]) -> Secrets {
        Secrets::new(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), secrecy::SecretString::from(*v)))
                .collect(),
            Some("test".into()),
        )
    }

    pub const PROFILES: &str = r##"
identity: { name: Jane, email: jane@example.com }
hats:
  normal:
    colour: "#2a2040"
    identity: { signing_key: { secret: git_signing_key } }
    env: { EDITOR: "code --wait" }
    path: ["~/.tenv/bin"]
  acme:
    inherits: normal
    colour: "#331420"
    identity: { name: Jane Doe, email: jane@acme.example }
    kube: { context: acme }
    env:
      JIRA_API_TOKEN: { secret: jira_token }
      JIRA_EMAIL: jane@acme.example
  plain:
    kube: { isolate: false }
"##;
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    fn plan(name: &str, opts: EnvOptions) -> EnvPlan {
        let cfg = config(PROFILES);
        let s = secrets(&[("git_signing_key", "SIGNKEY"), ("jira_token", "tok-123")]);
        EnvPlan::build(&cfg, name, &s, std::path::Path::new("/home/t"), opts).unwrap()
    }

    #[test]
    fn identity_becomes_the_four_git_variables() {
        let p = plan("acme", EnvOptions::default());
        assert_eq!(p.set["GIT_AUTHOR_NAME"], "Jane Doe");
        assert_eq!(p.set["GIT_COMMITTER_NAME"], "Jane Doe");
        assert_eq!(p.set["GIT_AUTHOR_EMAIL"], "jane@acme.example");
        assert_eq!(p.set["GIT_COMMITTER_EMAIL"], "jane@acme.example");
    }

    #[test]
    fn the_signing_key_is_injected_per_shell_via_git_config_variables() {
        let p = plan("acme", EnvOptions::default());
        assert_eq!(p.set["GIT_CONFIG_COUNT"], "1");
        assert_eq!(p.set["GIT_CONFIG_KEY_0"], "user.signingkey");
        assert_eq!(p.set["GIT_CONFIG_VALUE_0"], "SIGNKEY");
    }

    #[test]
    fn an_unfetched_signing_key_is_omitted_rather_than_set_empty() {
        let cfg = config(PROFILES);
        let p = EnvPlan::build(
            &cfg,
            "normal",
            &secrets(&[]),
            std::path::Path::new("/home/t"),
            EnvOptions::default(),
        )
        .unwrap();
        assert!(!p.set.contains_key("GIT_CONFIG_COUNT"));
        assert_eq!(p.missing_secrets, vec!["git_signing_key"]);
    }

    #[test]
    fn aws_points_at_this_hats_own_config_and_credentials() {
        let p = plan("acme", EnvOptions::default());
        assert_eq!(p.set["AWS_CONFIG_FILE"], "/home/t/.aws/.hats/acme.config");
        assert_eq!(
            p.set["AWS_SHARED_CREDENTIALS_FILE"],
            "/home/t/.aws/.hats/acme.credentials"
        );
    }

    /// A hand-exported AWS_PROFILE would select a section inside the next
    /// hat's isolated file, so a switch must clear it even though hats never
    /// sets it.
    #[test]
    fn a_stray_aws_profile_is_cleared_even_though_hats_never_sets_it() {
        let p = plan("acme", EnvOptions::default());
        assert!(!p.set.contains_key("AWS_PROFILE"));
        assert!(p.unset.contains(&"AWS_PROFILE".to_string()));
    }

    #[test]
    fn no_aws_leaves_the_shared_files_alone() {
        let p = plan(
            "acme",
            EnvOptions {
                no_aws: true,
                ..Default::default()
            },
        );
        assert!(!p.set.contains_key("AWS_CONFIG_FILE"));
        assert!(!p.set.contains_key("AWS_SHARED_CREDENTIALS_FILE"));
    }

    #[test]
    fn secret_references_resolve_and_literals_pass_through() {
        let p = plan("acme", EnvOptions::default());
        assert_eq!(p.set["JIRA_API_TOKEN"], "tok-123");
        assert_eq!(p.set["JIRA_EMAIL"], "jane@acme.example");
        assert_eq!(p.set["EDITOR"], "code --wait");
    }

    /// The regression guard: switching to a hat that sets none of the
    /// previous hat's variables must still clear them.
    #[test]
    fn the_unset_list_covers_variables_this_profile_never_sets() {
        let p = plan("normal", EnvOptions::default());
        assert!(!p.set.contains_key("JIRA_API_TOKEN"));
        assert!(p.unset.contains(&"JIRA_API_TOKEN".to_string()));
        assert!(p.unset.contains(&"JIRA_EMAIL".to_string()));
        assert!(p.unset.contains(&"AWS_PROFILE".to_string()));
    }

    #[test]
    fn the_unset_list_is_sorted_so_output_is_stable() {
        let mut sorted = plan("normal", EnvOptions::default()).unset;
        let original = sorted.clone();
        sorted.sort();
        assert_eq!(original, sorted);
    }

    #[test]
    fn the_profile_marker_is_set_last_and_cannot_be_overwritten() {
        let p = plan("acme", EnvOptions::default());
        let keys = p.exported();
        assert_eq!(keys[keys.len() - 1], "HATS_HAT");
        assert_eq!(p.set["HATS_HAT"], "acme");
    }

    #[test]
    fn kube_isolation_points_kubeconfig_at_a_per_profile_file() {
        let p = plan("acme", EnvOptions::default());
        assert_eq!(
            p.kubeconfig.as_deref(),
            Some(std::path::Path::new("/home/t/.kube/config.acme"))
        );
        assert_eq!(p.set["KUBECONFIG"], "/home/t/.kube/config.acme");
        assert_eq!(p.kube_context.as_deref(), Some("acme"));
    }

    #[test]
    fn a_profile_can_opt_out_of_kube_isolation() {
        let p = plan("plain", EnvOptions::default());
        assert!(p.kubeconfig.is_none());
        assert!(!p.set.contains_key("KUBECONFIG"));
        // But KUBECONFIG is still cleared, so it cannot leak in from elsewhere.
        assert!(p.unset.contains(&"KUBECONFIG".to_string()));
    }

    #[test]
    fn no_kube_suppresses_isolation_entirely() {
        let p = plan(
            "acme",
            EnvOptions {
                no_kube: true,
                ..Default::default()
            },
        );
        assert!(p.kubeconfig.is_none());
        assert!(p.kube_context.is_none());
    }

    #[test]
    fn no_colour_suppresses_the_tint() {
        assert_eq!(
            plan("acme", EnvOptions::default()).colour.as_deref(),
            Some("#331420")
        );
        assert!(
            plan(
                "acme",
                EnvOptions {
                    no_colour: true,
                    ..Default::default()
                }
            )
            .colour
            .is_none()
        );
    }

    #[test]
    fn reset_only_clears_without_setting_anything() {
        let p = plan(
            "acme",
            EnvOptions {
                reset_only: true,
                ..Default::default()
            },
        );
        assert!(p.set.is_empty());
        assert!(p.colour.is_none());
        assert!(!p.unset.is_empty());
    }

    #[test]
    fn tilde_paths_expand_against_the_given_home() {
        let p = plan("acme", EnvOptions::default());
        assert_eq!(p.path_prepend, vec!["/home/t/.tenv/bin"]);
        assert_eq!(expand_home("~", std::path::Path::new("/home/t")), "/home/t");
        assert_eq!(expand_home("/abs", std::path::Path::new("/home/t")), "/abs");
    }

    #[test]
    fn an_unknown_profile_is_an_error_naming_the_known_ones() {
        let cfg = config(PROFILES);
        let err = EnvPlan::build(
            &cfg,
            "ghost",
            &secrets(&[]),
            std::path::Path::new("/home/t"),
            EnvOptions::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("acme"));
    }
}
