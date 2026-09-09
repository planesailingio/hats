//! The secrets-backend abstraction.
//!
//! Only Bitwarden (including a self-hosted Vaultwarden) is implemented. AWS
//! Secrets Manager and HashiCorp Vault exist as registered-but-unimplemented
//! providers so the wizard can name them honestly and adding one later is a new
//! file rather than a refactor.

use std::collections::BTreeMap;

use anyhow::Result;
use secrecy::SecretString;

use crate::config::local::{ProviderKind, SecretsConfig};

/// Credentials a provider needs to authenticate. Held in memory only: on disk
/// they live inside the age envelope.
#[derive(Clone)]
pub struct Credentials {
    pub client_id: SecretString,
    pub client_secret: SecretString,
    /// Bitwarden's API key authenticates but does not decrypt; the master
    /// password is what unwraps the vault key.
    pub password: SecretString,
    /// The account email, needed for the KDF salt.
    pub email: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("email", &self.email)
            .finish_non_exhaustive()
    }
}

/// Whether a provider can run right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Ready,
    /// Configured but something is missing, with the fix.
    NotReady {
        why: String,
        fix: Option<String>,
    },
    /// Recognised but not built yet.
    NotImplemented,
}

/// A source of secrets.
pub trait Provider {
    fn kind(&self) -> ProviderKind;

    fn name(&self) -> &'static str {
        self.kind().as_str()
    }

    /// Can this provider run on this machine, as configured?
    fn availability(&self) -> Availability;

    /// Fetch every secret this provider can see, keyed by the names profiles
    /// and templates use.
    fn fetch(&self, creds: &Credentials) -> Result<BTreeMap<String, SecretString>>;
}

/// Build the provider named in the configuration.
pub fn for_config(cfg: &SecretsConfig) -> Box<dyn Provider> {
    match cfg.provider {
        ProviderKind::Bitwarden => Box::new(super::bitwarden::BitwardenProvider::new(
            cfg.bitwarden.clone().unwrap_or_default(),
        )),
        ProviderKind::AwsSecretsManager => Box::new(Unimplemented {
            kind: ProviderKind::AwsSecretsManager,
        }),
        ProviderKind::Vault => Box::new(Unimplemented {
            kind: ProviderKind::Vault,
        }),
        ProviderKind::None => Box::new(NoProvider),
    }
}

/// Placeholder for a provider that is named but not written.
pub struct Unimplemented {
    kind: ProviderKind,
}

impl Provider for Unimplemented {
    fn kind(&self) -> ProviderKind {
        self.kind
    }
    fn availability(&self) -> Availability {
        Availability::NotImplemented
    }
    fn fetch(&self, _creds: &Credentials) -> Result<BTreeMap<String, SecretString>> {
        anyhow::bail!(
            "the `{}` provider is not implemented yet. \
             Switch to `bitwarden`, or set provider: none in ~/.hats/config.yaml.",
            self.kind.as_str()
        )
    }
}

/// No backend: secrets stay empty and templates render without them.
pub struct NoProvider;

impl Provider for NoProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::None
    }
    fn availability(&self) -> Availability {
        Availability::NotReady {
            why: "no secrets provider is configured".into(),
            fix: Some("hats init --force".into()),
        }
    }
    fn fetch(&self, _creds: &Credentials) -> Result<BTreeMap<String, SecretString>> {
        Ok(BTreeMap::new())
    }
}

/// Turn a vault item name into a secret key: lowercase, non-alphanumerics
/// collapsed to underscores. Used by folder-based discovery so an item called
/// "Jira Acme API Token" becomes `jira_acme_api_token`.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_underscore = true; // suppress a leading underscore
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_matches_the_key_names_the_old_fetch_script_used() {
        assert_eq!(
            slugify("Jira Acme API Token"),
            "jira_acme_api_token"
        );
        assert_eq!(
            slugify("Chocolatey API Key"),
            "choco_api_key".replace("choco", "chocolatey")
        );
        assert_eq!(slugify("Git Signing Key ID"), "git_signing_key_id");
        assert_eq!(
            slugify("Authentik Bootstrap Token"),
            "authentik_bootstrap_token"
        );
    }

    #[test]
    fn slugify_collapses_runs_and_trims_edges() {
        assert_eq!(
            slugify("  Leading and   trailing  "),
            "leading_and_trailing"
        );
        assert_eq!(slugify("a--b__c"), "a_b_c");
        assert_eq!(slugify("Already_Fine"), "already_fine");
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn an_unimplemented_provider_says_so_rather_than_failing_obscurely() {
        let p = Unimplemented {
            kind: ProviderKind::Vault,
        };
        assert_eq!(p.availability(), Availability::NotImplemented);
        let err = p
            .fetch(&Credentials {
                client_id: SecretString::from("a"),
                client_secret: SecretString::from("b"),
                password: SecretString::from("c"),
                email: "t@example.com".into(),
            })
            .unwrap_err();
        assert!(err.to_string().contains("not implemented"), "{err}");
        assert!(
            err.to_string().contains("bitwarden"),
            "should name the way out"
        );
    }

    #[test]
    fn no_provider_fetches_nothing_without_erroring() {
        let creds = Credentials {
            client_id: SecretString::from("a"),
            client_secret: SecretString::from("b"),
            password: SecretString::from("c"),
            email: "t@example.com".into(),
        };
        assert!(NoProvider.fetch(&creds).unwrap().is_empty());
    }

    #[test]
    fn the_configured_provider_is_the_one_built() {
        let mut cfg = SecretsConfig::default();
        assert_eq!(for_config(&cfg).kind(), ProviderKind::None);
        cfg.provider = ProviderKind::Bitwarden;
        assert_eq!(for_config(&cfg).kind(), ProviderKind::Bitwarden);
        cfg.provider = ProviderKind::Vault;
        assert_eq!(for_config(&cfg).kind(), ProviderKind::Vault);
    }

    #[test]
    fn credentials_never_print_their_values() {
        let c = Credentials {
            client_id: SecretString::from("user.abc"),
            client_secret: SecretString::from("SUPERSECRET"),
            password: SecretString::from("hunter2"),
            email: "t@example.com".into(),
        };
        let shown = format!("{c:?}");
        assert!(shown.contains("t@example.com"));
        assert!(!shown.contains("SUPERSECRET"), "{shown}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}
