//! Bitwarden and Vaultwarden, via the official SDK crates.
//!
//! Vaultwarden does not implement Bitwarden's Secrets Manager (it is a licensed
//! feature and explicitly out of scope upstream), so hats reads the ordinary
//! personal vault: API-key login, sync, decrypt.
//!
//! **Discovery is by label, not by item name.** An item is claimed if it
//! carries a custom field named `hats` whose value is the secret key, or if it
//! sits in a folder named `hats` (the key is then the slugified item name).
//! That is what keeps vault item names out of the dotfiles repo: adding a
//! secret is a change in the vault, not a commit.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};

use super::provider::{Availability, Credentials, Provider, slugify};
use crate::config::local::{BitwardenConfig, ProviderKind};

/// Bitwarden's default cloud endpoints, used when nothing is self-hosted.
const CLOUD_API: &str = "https://api.bitwarden.com";
const CLOUD_IDENTITY: &str = "https://identity.bitwarden.com";

pub struct BitwardenProvider {
    cfg: BitwardenConfig,
}

impl BitwardenProvider {
    pub fn new(cfg: BitwardenConfig) -> Self {
        Self { cfg }
    }

    fn api_url(&self) -> String {
        self.cfg
            .api_url
            .clone()
            .unwrap_or_else(|| CLOUD_API.to_string())
    }

    fn identity_url(&self) -> String {
        self.cfg
            .identity_url
            .clone()
            .unwrap_or_else(|| CLOUD_IDENTITY.to_string())
    }
}

impl Provider for BitwardenProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Bitwarden
    }

    fn availability(&self) -> Availability {
        if self.cfg.self_hosted && self.cfg.api_url.is_none() {
            return Availability::NotReady {
                why: "self-hosted is set but no server URL is configured".into(),
                fix: Some("hats init --force".into()),
            };
        }
        Availability::Ready
    }

    fn fetch(&self, creds: &Credentials) -> Result<BTreeMap<String, SecretString>> {
        // The SDK is async; hats is not. One short-lived runtime per fetch is
        // simpler than colouring the whole CLI async for a single call.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("starting the async runtime for the Bitwarden client")?;
        runtime.block_on(self.fetch_async(creds))
    }
}

impl BitwardenProvider {
    async fn fetch_async(&self, creds: &Credentials) -> Result<BTreeMap<String, SecretString>> {
        use bitwarden_core::auth::login::ApiKeyLoginRequest;
        use bitwarden_core::{Client, ClientSettings, DeviceType};

        let settings = ClientSettings {
            identity_url: self.identity_url(),
            api_url: self.api_url(),
            user_agent: format!("hats/{}", crate::repo::BINARY_VERSION),
            device_type: DeviceType::SDK,
            // A stable per-machine identifier keeps the server from recording a
            // new device on every fetch.
            device_identifier: Some(device_identifier()),
            bitwarden_client_version: Some(crate::repo::BINARY_VERSION.to_string()),
            bitwarden_package_type: None,
        };
        let client = Client::new(Some(settings));

        client
            .auth()
            .login_api_key(&ApiKeyLoginRequest {
                client_id: creds.client_id.expose_secret().to_string(),
                client_secret: creds.client_secret.expose_secret().to_string(),
                password: creds.password.expose_secret().to_string(),
            })
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Bitwarden login failed: {e}. Check the API key and master password, \
                     and that {} is reachable.",
                    self.identity_url()
                )
            })?;

        // Login reports success even when the server returned no
        // MasterPasswordUnlock block, leaving an empty key store and failing
        // every decrypt later with a confusing message. Catch it here instead.
        let sync = fetch_sync(&client)
            .await
            .context("syncing the vault. If this says the key store is locked, the account may have no master password set.")?;

        let secrets = discover(&client, &sync, &self.cfg.discovery)?;
        Ok(secrets)
    }
}

/// A stable device id for this machine, derived from the hostname so it does
/// not change between runs and does not need storing.
fn device_identifier() -> String {
    let host = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "hats".into());
    // A UUID is expected; derive one deterministically from the hostname.
    let digest = crate::engine::state::hash(host.as_bytes());
    format!(
        "{}-{}-{}-{}-{}",
        &digest[0..8],
        &digest[8..12],
        &digest[12..16],
        &digest[16..20],
        &digest[20..32]
    )
}

/// The decrypted shape hats cares about, extracted from the sync response.
struct SyncView {
    folders: Vec<(String, String)>,
    ciphers: Vec<CipherView>,
}

struct CipherView {
    name: String,
    folder_id: Option<String>,
    /// Decrypted custom fields, name to value.
    fields: BTreeMap<String, String>,
    /// The login password or secure-note body, whichever this item has.
    primary: Option<String>,
}

async fn fetch_sync(client: &bitwarden_core::Client) -> Result<SyncView> {
    use bitwarden_api_api::apis::sync_api::{SyncApi, SyncApiClient};
    use bitwarden_crypto::{Decryptable, EncString};
    use std::sync::Arc;

    let configs = client.internal.get_api_configurations();
    let response = SyncApiClient::new(Arc::new(configs.api_config.clone()))
        .get(Some(true))
        .await
        .map_err(|e| anyhow::anyhow!("sync request failed: {e}"))?;

    let store = client.internal.get_key_store();
    let slot = bitwarden_core::key_management::SymmetricKeySlotId::User;

    // Decrypt one EncString, tolerating anything that will not decode so a
    // single odd item cannot break the whole fetch.
    let dec = |raw: &Option<String>| -> Option<String> {
        let text = raw.as_ref()?;
        let enc: EncString = text.parse().ok()?;
        enc.decrypt(&mut store.context(), slot).ok()
    };

    let folders = response
        .folders
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| Some((f.id?.to_string(), dec(&f.name)?)))
        .collect();

    let mut ciphers = Vec::new();
    for c in response.ciphers.unwrap_or_default() {
        let Some(name) = dec(&c.name) else { continue };

        let mut fields = BTreeMap::new();
        for field in c.fields.unwrap_or_default() {
            if let (Some(fname), value) = (dec(&field.name), dec(&field.value)) {
                fields.insert(fname, value.unwrap_or_default());
            }
        }

        let primary = c
            .login
            .as_ref()
            .and_then(|l| dec(&l.password))
            .or_else(|| dec(&c.notes));

        ciphers.push(CipherView {
            name,
            folder_id: c.folder_id.map(|id| id.to_string()),
            fields,
            primary,
        });
    }

    Ok(SyncView { folders, ciphers })
}

/// Map vault items to secret keys.
///
/// Split out and pure so the rules are testable without a server.
fn discover(
    _client: &bitwarden_core::Client,
    sync: &SyncView,
    discovery: &crate::config::local::DiscoveryConfig,
) -> Result<BTreeMap<String, SecretString>> {
    let folder_id = sync
        .folders
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(&discovery.folder))
        .map(|(id, _)| id.clone());

    let mut out: BTreeMap<String, SecretString> = BTreeMap::new();
    let mut sources: BTreeMap<String, String> = BTreeMap::new();

    for cipher in &sync.ciphers {
        let key = match cipher.fields.get(&discovery.field) {
            // A labelled item: the field's value is the secret key.
            Some(label) if !label.is_empty() => label.clone(),
            // Otherwise, an item in the hats folder, keyed by its name.
            _ if folder_id.is_some() && cipher.folder_id == folder_id => slugify(&cipher.name),
            _ => continue,
        };
        if key.is_empty() {
            continue;
        }

        // Which field holds the value: named explicitly, else the login
        // password or note body.
        let value = match cipher
            .fields
            .get(&format!("{}-value-field", discovery.field))
        {
            Some(field_name) => cipher.fields.get(field_name).cloned(),
            None => cipher.primary.clone(),
        };
        let Some(value) = value else { continue };

        if let Some(previous) = sources.get(&key) {
            anyhow::bail!(
                "two vault items both claim the secret `{key}`: \"{previous}\" and \"{}\". \
                 Remove the `{}` field from one of them.",
                cipher.name,
                discovery.field
            );
        }
        sources.insert(key.clone(), cipher.name.clone());
        out.insert(key, SecretString::from(value));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::local::DiscoveryConfig;

    fn cipher(
        name: &str,
        folder: Option<&str>,
        fields: &[(&str, &str)],
        primary: Option<&str>,
    ) -> CipherView {
        CipherView {
            name: name.into(),
            folder_id: folder.map(Into::into),
            fields: fields
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            primary: primary.map(Into::into),
        }
    }

    fn run(sync: SyncView) -> Result<BTreeMap<String, SecretString>> {
        // `discover` does not touch the client, so a real one is not needed.
        let client = bitwarden_core::Client::new(None);
        discover(&client, &sync, &DiscoveryConfig::default())
    }

    fn view(folders: &[(&str, &str)], ciphers: Vec<CipherView>) -> SyncView {
        SyncView {
            folders: folders
                .iter()
                .map(|(i, n)| ((*i).to_string(), (*n).to_string()))
                .collect(),
            ciphers,
        }
    }

    #[test]
    fn a_labelled_item_is_claimed_under_the_label_value() {
        let out = run(view(
            &[],
            vec![cipher(
                "Jira Acme API Token",
                None,
                &[("hats", "jira_acme_api_token")],
                Some("tok-123"),
            )],
        ))
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out["jira_acme_api_token"].expose_secret(), "tok-123");
    }

    #[test]
    fn an_item_in_the_hats_folder_is_keyed_by_its_slugified_name() {
        let out = run(view(
            &[("f1", "hats")],
            vec![cipher("Git Signing Key", Some("f1"), &[], Some("ABCD"))],
        ))
        .unwrap();
        assert_eq!(out["git_signing_key"].expose_secret(), "ABCD");
    }

    #[test]
    fn the_folder_name_match_is_case_insensitive() {
        let out = run(view(
            &[("f1", "Hats")],
            vec![cipher("Token", Some("f1"), &[], Some("v"))],
        ))
        .unwrap();
        assert!(out.contains_key("token"));
    }

    /// A vault full of unrelated logins must contribute nothing.
    #[test]
    fn unlabelled_items_outside_the_folder_are_ignored() {
        let out = run(view(
            &[("f1", "hats")],
            vec![
                cipher("Netflix", None, &[], Some("password")),
                cipher("Bank", Some("f2"), &[], Some("password")),
            ],
        ))
        .unwrap();
        assert!(
            out.is_empty(),
            "claimed something it should not: {:?}",
            out.keys()
        );
    }

    #[test]
    fn a_label_wins_over_folder_membership() {
        let out = run(view(
            &[("f1", "hats")],
            vec![cipher(
                "Some Item",
                Some("f1"),
                &[("hats", "explicit_key")],
                Some("v"),
            )],
        ))
        .unwrap();
        assert!(out.contains_key("explicit_key"));
        assert!(!out.contains_key("some_item"));
    }

    #[test]
    fn a_custom_field_can_name_which_field_holds_the_value() {
        let out = run(view(
            &[],
            vec![cipher(
                "Authentik",
                None,
                &[
                    ("hats", "authentik_token"),
                    ("hats-value-field", "bootstrap"),
                    ("bootstrap", "boot-value"),
                ],
                Some("the-login-password"),
            )],
        ))
        .unwrap();
        assert_eq!(out["authentik_token"].expose_secret(), "boot-value");
    }

    #[test]
    fn a_secure_note_body_is_used_when_there_is_no_login_password() {
        let out = run(view(
            &[],
            vec![cipher(
                "Note",
                None,
                &[("hats", "note_key")],
                Some("body text"),
            )],
        ))
        .unwrap();
        assert_eq!(out["note_key"].expose_secret(), "body text");
    }

    #[test]
    fn an_item_with_no_value_is_skipped_rather_than_stored_empty() {
        let out = run(view(
            &[],
            vec![cipher("Empty", None, &[("hats", "empty_key")], None)],
        ))
        .unwrap();
        assert!(out.is_empty());
    }

    /// Two items claiming one key would make the fetch order-dependent, so it
    /// is an error naming both.
    #[test]
    fn duplicate_claims_are_an_error_naming_both_items() {
        let err = run(view(
            &[],
            vec![
                cipher("Old Token", None, &[("hats", "jira_token")], Some("a")),
                cipher("New Token", None, &[("hats", "jira_token")], Some("b")),
            ],
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Old Token"), "{msg}");
        assert!(msg.contains("New Token"), "{msg}");
    }

    #[test]
    fn an_empty_label_does_not_claim_the_item() {
        let out = run(view(
            &[],
            vec![cipher("Thing", None, &[("hats", "")], Some("v"))],
        ))
        .unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn self_hosted_without_a_url_is_reported_as_not_ready() {
        let p = BitwardenProvider::new(BitwardenConfig {
            self_hosted: true,
            api_url: None,
            identity_url: None,
            discovery: DiscoveryConfig::default(),
        });
        assert!(matches!(p.availability(), Availability::NotReady { .. }));
    }

    #[test]
    fn endpoints_default_to_the_cloud_when_not_self_hosted() {
        let p = BitwardenProvider::new(BitwardenConfig::default());
        assert_eq!(p.api_url(), CLOUD_API);
        assert_eq!(p.identity_url(), CLOUD_IDENTITY);
        assert_eq!(p.availability(), Availability::Ready);
    }

    #[test]
    fn configured_endpoints_are_used_verbatim() {
        let p = BitwardenProvider::new(BitwardenConfig {
            self_hosted: true,
            api_url: Some("https://vault.example.net/api".into()),
            identity_url: Some("https://vault.example.net/identity".into()),
            discovery: DiscoveryConfig::default(),
        });
        assert_eq!(p.api_url(), "https://vault.example.net/api");
        assert_eq!(p.identity_url(), "https://vault.example.net/identity");
    }
}
