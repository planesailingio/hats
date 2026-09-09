//! `~/.hats/config.yaml` — everything specific to this machine and this human.
//!
//! Profiles live here, not in the repo, so the dotfiles repo can be public and
//! generic while client names, emails, AWS profiles and kube contexts stay on
//! the laptop. The repo ships defaults (zsh, starship, themes); this file says
//! who you are.

use std::path::Path;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::config::profile::{IdentitySpec, ProfileSpec};
use crate::error::ConfigError;

const HEADER: &str = "\
# ~/.hats/config.yaml — machine-local configuration, written by `hats init`.
#
# This file is NOT in the dotfiles repo. Profiles, identities and provider
# endpoints stay on this machine. Edit by hand or with `hats profile edit`.
";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalConfig {
    #[serde(default)]
    pub hats: LocalMeta,
    /// Fallback identity: fills any field the base profile leaves unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentitySpec>,
    /// Wizard answers, one per group declared in the repo manifest.
    #[serde(default)]
    pub groups: IndexMap<String, bool>,
    #[serde(default)]
    pub profiles: IndexMap<String, ProfileSpec>,
    #[serde(default)]
    pub secrets: SecretsConfig,
    /// Free-form values exposed to templates as `machine.*`.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub machine: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalMeta {
    /// Clone URL for the dotfiles repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Profile loaded on shell start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
}

/// Which secrets backend to use. Only Bitwarden is implemented; the other
/// variants exist so the wizard can name them and the `Provider` trait has
/// somewhere to grow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    /// Bitwarden or a self-hosted Vaultwarden.
    Bitwarden,
    /// AWS Secrets Manager. Not implemented yet.
    AwsSecretsManager,
    /// HashiCorp Vault. Not implemented yet.
    Vault,
    /// No provider: secrets stay empty and templates render without them.
    #[default]
    None,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Bitwarden => "bitwarden",
            ProviderKind::AwsSecretsManager => "aws-secrets-manager",
            ProviderKind::Vault => "vault",
            ProviderKind::None => "none",
        }
    }

    pub fn implemented(self) -> bool {
        matches!(self, ProviderKind::Bitwarden | ProviderKind::None)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretsConfig {
    #[serde(default)]
    pub provider: ProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitwarden: Option<BitwardenConfig>,
    #[serde(default)]
    pub envelope: EnvelopeConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BitwardenConfig {
    /// True for Vaultwarden or any self-hosted instance.
    #[serde(default)]
    pub self_hosted: bool,
    /// e.g. https://vault.example.net/api
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    /// e.g. https://vault.example.net/identity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_url: Option<String>,
    #[serde(default)]
    pub discovery: DiscoveryConfig,
}

/// How hats finds its secrets in a vault full of unrelated logins.
///
/// An item is claimed if it carries a custom field named `field` (whose value
/// is the secret key), or if it sits in the folder named `folder` (the key is
/// then the slugified item name). The repo never lists item names, so a second
/// machine needs no manifest edit to add a secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    #[serde(default = "default_field")]
    pub field: String,
    #[serde(default = "default_folder")]
    pub folder: String,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            field: default_field(),
            folder: default_folder(),
        }
    }
}

fn default_field() -> String {
    "hats".into()
}

fn default_folder() -> String {
    "hats".into()
}

/// How the provider credentials are protected at rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnvelopeMethod {
    /// age, encrypted to a key held in a YubiKey PIV slot.
    YubikeyPiv,
    /// Nothing stored: every fetch prompts for the credentials.
    #[default]
    None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvelopeConfig {
    #[serde(default)]
    pub method: EnvelopeMethod,
    /// Public age recipient, e.g. age1yubikey1...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,
    /// PIV slot the key lives in (retired slots are 82-95).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

impl LocalConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::NotInitialised);
        }
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        serde_yaml_ng::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Write atomically: render to a sibling temp file, then rename, so an
    /// interrupted write cannot leave a half-parsed config behind.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let body = serde_yaml_ng::to_string(self).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        let contents = format!("{HEADER}{body}");

        let parent = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
        let tmp = path.with_extension("yaml.tmp");
        std::fs::write(&tmp, contents).map_err(|source| ConfigError::Write {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, path).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    /// The profile a new shell starts in: the configured default, else the
    /// first profile declared, else "normal".
    pub fn default_profile(&self) -> String {
        self.hats
            .default_profile
            .clone()
            .or_else(|| self.profiles.keys().next().cloned())
            .unwrap_or_else(|| "normal".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile::EnvValue;

    #[test]
    fn round_trips_through_yaml() {
        let mut cfg = LocalConfig::default();
        cfg.hats.repo = Some("https://example.com/dotfiles.git".into());
        cfg.hats.default_profile = Some("normal".into());
        cfg.identity = Some(IdentitySpec {
            name: Some("Jane".into()),
            email: Some("jane@example.com".into()),
            signing_key: None,
        });
        cfg.groups.insert("shell".into(), true);
        cfg.groups.insert("env-bundles".into(), false);
        let mut normal = ProfileSpec {
            colour: Some("#2a2040".into()),
            ..Default::default()
        };
        normal
            .env
            .insert("EDITOR".into(), EnvValue::Literal("code --wait".into()));
        cfg.profiles.insert("normal".into(), normal);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        cfg.save(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# ~/.hats/config.yaml"), "header missing");

        let back = LocalConfig::load(&path).unwrap();
        assert_eq!(back.default_profile(), "normal");
        assert!(!back.groups["env-bundles"]);
        assert_eq!(
            back.profiles["normal"].env["EDITOR"],
            EnvValue::Literal("code --wait".into())
        );
    }

    #[test]
    fn a_missing_config_reports_not_initialised() {
        let dir = tempfile::tempdir().unwrap();
        let err = LocalConfig::load(&dir.path().join("nope.yaml")).unwrap_err();
        assert!(matches!(err, ConfigError::NotInitialised));
        assert!(err.to_string().contains("hats init"));
    }

    #[test]
    fn default_profile_falls_back_to_the_first_declared() {
        let mut cfg = LocalConfig::default();
        cfg.profiles.insert("work".into(), ProfileSpec::default());
        cfg.profiles.insert("home".into(), ProfileSpec::default());
        assert_eq!(cfg.default_profile(), "work");
    }

    #[test]
    fn default_profile_falls_back_to_normal_when_there_are_none() {
        assert_eq!(LocalConfig::default().default_profile(), "normal");
    }

    #[test]
    fn provider_kinds_report_what_is_implemented() {
        assert!(ProviderKind::Bitwarden.implemented());
        assert!(ProviderKind::None.implemented());
        assert!(!ProviderKind::Vault.implemented());
        assert_eq!(
            ProviderKind::AwsSecretsManager.as_str(),
            "aws-secrets-manager"
        );
    }

    #[test]
    fn discovery_defaults_to_the_hats_field_and_folder() {
        let d = DiscoveryConfig::default();
        assert_eq!(d.field, "hats");
        assert_eq!(d.folder, "hats");
    }

    #[test]
    fn saving_is_atomic_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        LocalConfig::default().save(&path).unwrap();
        assert!(path.is_file());
        assert!(!path.with_extension("yaml.tmp").exists());
    }
}
