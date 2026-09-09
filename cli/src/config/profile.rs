//! Profiles as data.
//!
//! The old system wrote one zsh file per client that `source`d its parent and
//! relied on a hand-maintained `_profile_reset` unset list. That list was
//! incomplete, so `AWS_PROFILE`, `JIRA_API_TOKEN` and friends leaked from one
//! profile into the next.
//!
//! Here a profile is a struct. Inheritance is a fold over the chain, and the
//! unset list is [`env_keys`](ResolvedProfile::env_keys) unioned across *every*
//! profile. Adding a variable to any profile therefore adds it to the reset set
//! by construction: the leak cannot come back.

use std::collections::BTreeSet;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// Environment variables hats always owns, whichever profile is active.
/// `DEV_PROFILE` is kept as a compatibility alias for starship and the
/// `~/.ssh/config.d` `Match exec` recipe. `ENV_PROFILE` belongs to the
/// orthogonal dev/staging/prod bundles, and is reset so a bundle cannot
/// outlive the profile switch that follows it.
pub const ALWAYS_OWNED: &[&str] = &["HATS_PROFILE", "DEV_PROFILE", "ENV_PROFILE", "KUBECONFIG"];

/// A value in a profile's `env` map: either a literal or a reference to a
/// secret resolved at emit time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    Literal(String),
    Secret { secret: String },
}

impl EnvValue {
    pub fn secret_ref(&self) -> Option<&str> {
        match self {
            EnvValue::Secret { secret } => Some(secret),
            EnvValue::Literal(_) => None,
        }
    }
}

/// Git identity for a profile. Every field is optional so a child profile can
/// override the email while inheriting the signing key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentitySpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<EnvValue>,
}

impl IdentitySpec {
    fn merge(&mut self, child: &IdentitySpec) {
        if child.name.is_some() {
            self.name = child.name.clone();
        }
        if child.email.is_some() {
            self.email = child.email.clone();
        }
        if child.signing_key.is_some() {
            self.signing_key = child.signing_key.clone();
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AwsSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

impl AwsSpec {
    fn merge(&mut self, child: &AwsSpec) {
        if child.profile.is_some() {
            self.profile = child.profile.clone();
        }
        if child.region.is_some() {
            self.region = child.region.clone();
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KubeSpec {
    /// Context to select in this profile's own kubeconfig.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Give this profile its own copy of ~/.kube/config. Defaults to true:
    /// even the base profile isolates, so a plain terminal never writes the
    /// shared file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolate: Option<bool>,
}

impl KubeSpec {
    fn merge(&mut self, child: &KubeSpec) {
        if child.context.is_some() {
            self.context = child.context.clone();
        }
        if child.isolate.is_some() {
            self.isolate = child.isolate;
        }
    }
}

/// A profile exactly as written in `~/.hats/config.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Terminal background tint, e.g. "#331420", or "reset".
    #[serde(alias = "color", skip_serializing_if = "Option::is_none")]
    pub colour: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentitySpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws: Option<AwsSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kube: Option<KubeSpec>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, EnvValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
}

/// A profile with its inheritance chain folded in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedProfile {
    pub name: String,
    pub description: Option<String>,
    pub colour: Option<String>,
    pub identity: IdentitySpec,
    pub aws: AwsSpec,
    pub kube: KubeSpec,
    pub env: IndexMap<String, EnvValue>,
    pub path: Vec<String>,
}

impl ResolvedProfile {
    /// Whether this profile gets its own kubeconfig. Isolation is the default
    /// because the whole point of the system is that a `use-context` in one
    /// terminal cannot leak into another.
    pub fn kube_isolated(&self) -> bool {
        self.kube.isolate.unwrap_or(true)
    }

    /// Every environment variable this profile sets, derived plus explicit.
    ///
    /// This is the function the reset list is built from, so anything that
    /// [`crate::profile`] emits must be reported here or it will leak.
    pub fn env_keys(&self) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        for k in ALWAYS_OWNED {
            keys.insert((*k).to_string());
        }
        if self.identity.name.is_some() {
            keys.insert("GIT_AUTHOR_NAME".into());
            keys.insert("GIT_COMMITTER_NAME".into());
        }
        if self.identity.email.is_some() {
            keys.insert("GIT_AUTHOR_EMAIL".into());
            keys.insert("GIT_COMMITTER_EMAIL".into());
        }
        if self.identity.signing_key.is_some() {
            keys.insert("GIT_CONFIG_COUNT".into());
            keys.insert("GIT_CONFIG_KEY_0".into());
            keys.insert("GIT_CONFIG_VALUE_0".into());
        }
        if self.aws.profile.is_some() {
            keys.insert("AWS_PROFILE".into());
        }
        if self.aws.region.is_some() {
            keys.insert("AWS_REGION".into());
            keys.insert("AWS_DEFAULT_REGION".into());
        }
        keys.extend(self.env.keys().cloned());
        keys
    }

    /// Secret keys this profile refers to, so `hats lint` can flag a reference
    /// to a secret the provider never supplies.
    pub fn secret_refs(&self) -> BTreeSet<String> {
        let mut refs = BTreeSet::new();
        if let Some(v) = self
            .identity
            .signing_key
            .as_ref()
            .and_then(EnvValue::secret_ref)
        {
            refs.insert(v.to_string());
        }
        for v in self.env.values() {
            if let Some(s) = v.secret_ref() {
                refs.insert(s.to_string());
            }
        }
        refs
    }
}

/// Fold a profile's inheritance chain into a [`ResolvedProfile`].
///
/// `base_identity` is the top-level `identity:` from the machine config: it
/// fills any field the root profile leaves unset, so a second user only has to
/// answer the wizard rather than edit every profile.
pub fn resolve(
    name: &str,
    profiles: &IndexMap<String, ProfileSpec>,
    base_identity: Option<&IdentitySpec>,
) -> Result<ResolvedProfile, ConfigError> {
    // Walk up to the root, detecting cycles and missing parents.
    let mut chain: Vec<&str> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut cursor = name;
    loop {
        if !seen.insert(cursor) {
            let mut cycle: Vec<String> = chain.iter().rev().map(|s| (*s).to_string()).collect();
            cycle.push(cursor.to_string());
            return Err(ConfigError::ProfileCycle {
                profile: name.to_string(),
                chain: cycle.join(" -> "),
            });
        }
        let spec = profiles.get(cursor).ok_or_else(|| {
            if cursor == name {
                ConfigError::UnknownProfile {
                    name: name.to_string(),
                    known: profiles.keys().cloned().collect(),
                }
            } else {
                ConfigError::UnknownParent {
                    profile: name.to_string(),
                    parent: cursor.to_string(),
                }
            }
        })?;
        chain.push(cursor);
        match spec.inherits.as_deref() {
            Some(parent) => cursor = parent,
            None => break,
        }
    }

    let mut out = ResolvedProfile {
        name: name.to_string(),
        description: None,
        colour: None,
        identity: base_identity.cloned().unwrap_or_default(),
        aws: AwsSpec::default(),
        kube: KubeSpec::default(),
        env: IndexMap::new(),
        path: Vec::new(),
    };

    // Root first, leaf last, so the child always wins.
    for step in chain.iter().rev() {
        let spec = &profiles[*step];
        if spec.description.is_some() {
            out.description = spec.description.clone();
        }
        if spec.colour.is_some() {
            out.colour = spec.colour.clone();
        }
        if let Some(id) = &spec.identity {
            out.identity.merge(id);
        }
        if let Some(aws) = &spec.aws {
            out.aws.merge(aws);
        }
        if let Some(kube) = &spec.kube {
            out.kube.merge(kube);
        }
        for (k, v) in &spec.env {
            out.env.insert(k.clone(), v.clone());
        }
        for p in &spec.path {
            if !out.path.contains(p) {
                out.path.push(p.clone());
            }
        }
    }

    // The description of an inherited profile should not be the parent's.
    if profiles[name].description.is_none() {
        out.description = profiles[name].description.clone();
    }

    Ok(out)
}

/// The union of every profile's variables: the unset list emitted before any
/// profile's exports. Unioning across *all* profiles (not just the one being
/// switched to) is what stops the previous profile leaking into the next.
pub fn all_env_keys(
    profiles: &IndexMap<String, ProfileSpec>,
    base_identity: Option<&IdentitySpec>,
) -> BTreeSet<String> {
    let mut keys: BTreeSet<String> = ALWAYS_OWNED.iter().map(|s| (*s).to_string()).collect();
    for name in profiles.keys() {
        // A broken profile must not silently shrink the reset list, but it is
        // reported by `hats lint` rather than blocking a shell switch, so fall
        // back to the raw keys we can see without resolving.
        match resolve(name, profiles, base_identity) {
            Ok(p) => keys.extend(p.env_keys()),
            Err(_) => keys.extend(profiles[name].env.keys().cloned()),
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(yaml: &str) -> IndexMap<String, ProfileSpec> {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    fn fixture() -> IndexMap<String, ProfileSpec> {
        spec(
            r##"
normal:
  colour: "#2a2040"
  identity: { name: Jane, email: jane@example.com, signing_key: { secret: git_signing_key } }
  aws: { profile: default, region: eu-west-2 }
  kube: { isolate: true }
  env: { EDITOR: "code --wait" }
  path: ["~/.tenv/bin"]
acme:
  inherits: normal
  colour: "#331420"
  identity: { name: Jane Doe, email: jane@acme.example }
  aws: { profile: acme-aws }
  kube: { context: acme }
  env:
    JIRA_API_TOKEN: { secret: jira_token }
    JIRA_EMAIL: jane@acme.example
  path: ["~/.acme/bin"]
"##,
        )
    }

    #[test]
    fn child_overrides_parent_and_inherits_the_rest() {
        let p = resolve("acme", &fixture(), None).unwrap();
        assert_eq!(p.identity.email.as_deref(), Some("jane@acme.example"));
        // region and signing key come from normal
        assert_eq!(p.aws.region.as_deref(), Some("eu-west-2"));
        assert_eq!(p.aws.profile.as_deref(), Some("acme-aws"));
        assert_eq!(
            p.identity.signing_key,
            Some(EnvValue::Secret {
                secret: "git_signing_key".into()
            })
        );
        // env unions, child wins; path concatenates parent-then-child
        assert_eq!(p.env.len(), 3);
        assert_eq!(p.path, vec!["~/.tenv/bin", "~/.acme/bin"]);
        assert_eq!(p.colour.as_deref(), Some("#331420"));
    }

    #[test]
    fn base_identity_fills_gaps_in_the_root_profile() {
        let profiles = spec("normal:\n  aws: { profile: default }\n");
        let base = IdentitySpec {
            name: Some("Jane".into()),
            email: Some("jane@example.com".into()),
            signing_key: None,
        };
        let p = resolve("normal", &profiles, Some(&base)).unwrap();
        assert_eq!(p.identity.name.as_deref(), Some("Jane"));
    }

    #[test]
    fn a_cycle_is_an_error_naming_the_chain() {
        let profiles = spec("a:\n  inherits: b\nb:\n  inherits: a\n");
        let err = resolve("a", &profiles, None).unwrap_err();
        assert!(matches!(err, ConfigError::ProfileCycle { .. }), "{err:?}");
    }

    #[test]
    fn a_missing_parent_names_the_parent_not_the_child() {
        let profiles = spec("a:\n  inherits: ghost\n");
        let err = resolve("a", &profiles, None).unwrap_err();
        match err {
            ConfigError::UnknownParent { parent, .. } => assert_eq!(parent, "ghost"),
            other => panic!("expected UnknownParent, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_profile_lists_the_known_ones() {
        let err = resolve("nope", &fixture(), None).unwrap_err();
        match err {
            ConfigError::UnknownProfile { known, .. } => {
                assert_eq!(known, vec!["normal", "acme"]);
            }
            other => panic!("expected UnknownProfile, got {other:?}"),
        }
    }

    #[test]
    fn env_keys_include_the_derived_git_and_aws_variables() {
        let p = resolve("acme", &fixture(), None).unwrap();
        let keys = p.env_keys();
        for expected in [
            "GIT_AUTHOR_NAME",
            "GIT_COMMITTER_EMAIL",
            "GIT_CONFIG_VALUE_0",
            "AWS_PROFILE",
            "AWS_DEFAULT_REGION",
            "JIRA_API_TOKEN",
            "HATS_PROFILE",
            "KUBECONFIG",
        ] {
            assert!(keys.contains(expected), "missing {expected}");
        }
    }

    /// The regression test for the original bug: switching to a profile that
    /// does not set JIRA_API_TOKEN must still unset it.
    #[test]
    fn the_reset_list_is_the_union_over_every_profile() {
        let profiles = fixture();
        let all = all_env_keys(&profiles, None);
        let normal = resolve("normal", &profiles, None).unwrap();
        assert!(!normal.env_keys().contains("JIRA_API_TOKEN"));
        assert!(all.contains("JIRA_API_TOKEN"));
        assert!(all.contains("JIRA_EMAIL"));
    }

    /// Adding a variable to any one profile must widen the reset list with no
    /// other edit. This is the property that keeps the leak fixed.
    #[test]
    fn a_new_variable_joins_the_reset_list_automatically() {
        let mut profiles = fixture();
        let before = all_env_keys(&profiles, None);
        assert!(!before.contains("NEW_TOKEN"));
        profiles
            .get_mut("acme")
            .unwrap()
            .env
            .insert("NEW_TOKEN".into(), EnvValue::Literal("x".into()));
        assert!(all_env_keys(&profiles, None).contains("NEW_TOKEN"));
    }

    #[test]
    fn secret_refs_cover_env_and_the_signing_key() {
        let p = resolve("acme", &fixture(), None).unwrap();
        let refs = p.secret_refs();
        assert!(refs.contains("git_signing_key"));
        assert!(refs.contains("jira_token"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn kube_isolation_defaults_to_on() {
        let profiles = spec("solo: {}\n");
        assert!(resolve("solo", &profiles, None).unwrap().kube_isolated());
    }

    #[test]
    fn colour_accepts_the_american_spelling_too() {
        let profiles = spec("solo:\n  color: \"#112233\"\n");
        let p = resolve("solo", &profiles, None).unwrap();
        assert_eq!(p.colour.as_deref(), Some("#112233"));
    }
}
