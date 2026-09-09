//! `~/.hats/secrets.yaml` — the one plaintext copy of the fetched secrets.
//!
//! The old system baked secret values into five rendered dotfiles at apply
//! time. That kept shells offline and instant, which is the right property, but
//! scattered the plaintext. One 0600 file keeps the property and shrinks the
//! blast radius to a single path.
//!
//! Values are wrapped in [`SecretString`] so they cannot be printed by
//! accident: `Debug` on this type shows key names only.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

/// Placeholder substituted for a secret when rendering for a diff.
pub fn placeholder(key: &str) -> String {
    format!("«secret:{key}»")
}

#[derive(Debug, Serialize, Deserialize)]
struct SecretsFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    fetched_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default)]
    values: BTreeMap<String, String>,
}

/// Fetched secret values, keyed by the names hats and templates use.
#[derive(Default, Clone)]
pub struct Secrets {
    values: BTreeMap<String, SecretString>,
    pub fetched_at: Option<String>,
    pub provider: Option<String>,
}

impl std::fmt::Debug for Secrets {
    /// Never print values, not even in a panic message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets")
            .field("keys", &self.values.keys().collect::<Vec<_>>())
            .field("fetched_at", &self.fetched_at)
            .finish()
    }
}

impl Secrets {
    pub fn new(values: BTreeMap<String, SecretString>, provider: Option<String>) -> Self {
        Self {
            values,
            fetched_at: Some(chrono::Utc::now().to_rfc3339()),
            provider,
        }
    }

    /// Load the file, or an empty set when it does not exist yet.
    ///
    /// A missing file is normal (no provider configured, or nothing fetched),
    /// so it is not an error: templates render with empty values and the plan
    /// says so in its header.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let file: SecretsFile = serde_yaml_ng::from_str(&text)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(Self {
            values: file
                .values
                .into_iter()
                .map(|(k, v)| (k, SecretString::from(v)))
                .collect(),
            fetched_at: file.fetched_at,
            provider: file.provider,
        })
    }

    /// Write atomically at 0600. The mode is set on the temp file *before* the
    /// rename, so the secrets are never briefly world-readable.
    pub fn save(&self, path: &Path) -> Result<()> {
        let file = SecretsFile {
            fetched_at: self.fetched_at.clone(),
            provider: self.provider.clone(),
            values: self
                .values
                .iter()
                .map(|(k, v)| (k.clone(), v.expose_secret().to_string()))
                .collect(),
        };
        let body = serde_yaml_ng::to_string(&file)?;
        let contents = format!(
            "# ~/.hats/secrets.yaml — fetched secrets. Mode 0600, never in git.\n\
             # Written by `hats secrets fetch`; do not edit by hand.\n{body}"
        );

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("yaml.tmp");
        std::fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
        set_owner_only(&tmp)?;
        std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&SecretString> {
        self.values.get(key)
    }

    /// The value, or an empty string. Templates always render, so a missing
    /// secret degrades to an empty variable rather than a failed apply.
    pub fn value_or_empty(&self, key: &str) -> String {
        self.values
            .get(key)
            .map(|v| v.expose_secret().to_string())
            .unwrap_or_default()
    }

    pub fn keys(&self) -> Vec<&str> {
        self.values.keys().map(String::as_str).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Which of `wanted` are absent or blank, for `hats doctor` and the plan
    /// header.
    pub fn missing<'a>(&self, wanted: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        wanted
            .into_iter()
            .filter(|k| {
                self.values
                    .get(*k)
                    .is_none_or(|v| v.expose_secret().is_empty())
            })
            .map(str::to_owned)
            .collect()
    }

    /// Every non-trivial value, for the redactor. Values shorter than four
    /// characters are skipped: masking "1" would mangle unrelated output.
    pub fn maskable(&self) -> Vec<(String, String)> {
        self.values
            .iter()
            .map(|(k, v)| (k.clone(), v.expose_secret().to_string()))
            .filter(|(_, v)| v.len() >= 4)
            .collect()
    }
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting 0600 on {}", path.display()))
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Secrets {
        let mut m = BTreeMap::new();
        m.insert(
            "git_signing_key".to_string(),
            SecretString::from("ABCD1234"),
        );
        m.insert("jira_token".to_string(), SecretString::from("tok-xyz"));
        m.insert("tiny".to_string(), SecretString::from("ab"));
        Secrets::new(m, Some("bitwarden".into()))
    }

    #[test]
    fn round_trips_and_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.yaml");
        sample().save(&path).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "secrets file must be owner-only");
        }

        let back = Secrets::load(&path).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back.value_or_empty("jira_token"), "tok-xyz");
        assert_eq!(back.provider.as_deref(), Some("bitwarden"));
        assert!(!path.with_extension("yaml.tmp").exists());
    }

    #[test]
    fn a_missing_file_is_an_empty_set_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let s = Secrets::load(&dir.path().join("absent.yaml")).unwrap();
        assert!(s.is_empty());
        assert_eq!(s.value_or_empty("anything"), "");
    }

    #[test]
    fn debug_never_reveals_a_value() {
        let rendered = format!("{:?}", sample());
        assert!(
            rendered.contains("git_signing_key"),
            "keys should be visible"
        );
        assert!(
            !rendered.contains("ABCD1234"),
            "values must not be: {rendered}"
        );
    }

    #[test]
    fn missing_reports_absent_and_blank_keys() {
        let mut m = BTreeMap::new();
        m.insert("set".to_string(), SecretString::from("value"));
        m.insert("blank".to_string(), SecretString::from(""));
        let s = Secrets::new(m, None);
        assert_eq!(
            s.missing(["set", "blank", "absent"]),
            vec!["blank", "absent"]
        );
    }

    #[test]
    fn maskable_skips_values_too_short_to_mask_safely() {
        let keys: Vec<String> = sample().maskable().into_iter().map(|(k, _)| k).collect();
        assert!(keys.contains(&"jira_token".to_string()));
        assert!(!keys.contains(&"tiny".to_string()));
    }

    #[test]
    fn the_placeholder_names_the_key() {
        assert_eq!(placeholder("jira_token"), "«secret:jira_token»");
    }
}
