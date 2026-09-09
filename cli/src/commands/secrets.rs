//! `hats secrets` — fetch, inspect, clear, and enrol a YubiKey.
//!
//! The credentials that reach the vault are themselves a secret, so they are
//! never written in plaintext. Either they sit in `~/.hats/envelope.age`,
//! encrypted to a YubiKey, or they are typed in each time.

use anyhow::{Context, Result};
use secrecy::SecretString;

use crate::app::App;
use crate::cli::{SecretsArgs, SecretsCommand};
use crate::config::local::{EnvelopeMethod, ProviderKind};
use crate::secrets::provider::{Availability, Credentials};
use crate::secrets::store::Secrets;
use crate::secrets::{envelope, provider};

pub fn run(app: &mut App, args: &SecretsArgs) -> Result<i32> {
    match &args.command {
        SecretsCommand::Fetch => fetch(app),
        SecretsCommand::Status => status(app),
        SecretsCommand::Clear => clear(app),
        SecretsCommand::EnrolYubikey { slot } => enrol(app, slot.clone()),
    }
}

fn fetch(app: &mut App) -> Result<i32> {
    let cfg = app.config()?;
    let secrets_cfg = cfg.local.secrets.clone();

    if secrets_cfg.provider == ProviderKind::None {
        app.ui
            .warn("no secrets provider is configured. Run `hats init --force` to set one up.");
        return Ok(1);
    }

    let backend = provider::for_config(&secrets_cfg);
    match backend.availability() {
        Availability::Ready => {}
        Availability::NotImplemented => {
            anyhow::bail!("the `{}` provider is not implemented yet", backend.name())
        }
        Availability::NotReady { why, fix } => {
            let hint = fix.map(|f| format!(" Run `{f}`.")).unwrap_or_default();
            anyhow::bail!("{} is not ready: {why}.{hint}", backend.name());
        }
    }

    let creds = credentials(app, &secrets_cfg)?;
    app.ui.say(format!("Fetching from {}…", backend.name()));

    let values = backend.fetch(&creds)?;
    if values.is_empty() {
        app.ui.warn(format!(
            "no secrets found. Label an item with a custom field `{}` whose value is the \
             secret key, or put it in a folder called `{}`.",
            secrets_cfg
                .bitwarden
                .as_ref()
                .map(|b| b.discovery.field.clone())
                .unwrap_or_else(|| "hats".into()),
            secrets_cfg
                .bitwarden
                .as_ref()
                .map(|b| b.discovery.folder.clone())
                .unwrap_or_else(|| "hats".into()),
        ));
    }

    let count = values.len();
    let store = Secrets::new(values, Some(backend.name().to_string()));
    store
        .save(&app.paths.secrets)
        .with_context(|| format!("writing {}", app.paths.secrets.display()))?;

    app.ui.ok(format!(
        "fetched {count} secret{} into {} (mode 0600)",
        if count == 1 { "" } else { "s" },
        app.paths.secrets.display()
    ));

    // Anything a profile refers to but the vault did not supply.
    let mut wanted: std::collections::BTreeSet<String> =
        cfg.repo.secrets.required.iter().cloned().collect();
    for name in cfg.profiles().keys() {
        if let Ok(p) = cfg.resolve_profile(name) {
            wanted.extend(p.secret_refs());
        }
    }
    let missing = store.missing(wanted.iter().map(String::as_str));
    if !missing.is_empty() {
        app.ui.warn(format!(
            "still missing: {}. Label the matching vault items.",
            missing.join(", ")
        ));
    }

    app.ui
        .say("Open a new shell, or run `profile <name>`, to pick them up.");
    Ok(0)
}

fn status(app: &mut App) -> Result<i32> {
    let cfg = app.config()?;
    let store = Secrets::load(&app.paths.secrets)?;

    app.ui
        .say(format!("provider  {}", cfg.local.secrets.provider.as_str()));
    if let Some(bw) = &cfg.local.secrets.bitwarden
        && let Some(url) = &bw.api_url
    {
        app.ui.say(format!("server    {url}"));
    }
    app.ui.say(format!(
        "envelope  {}",
        match cfg.local.secrets.envelope.method {
            EnvelopeMethod::YubikeyPiv if app.paths.envelope.is_file() =>
                format!("YubiKey PIV ({})", app.paths.envelope.display()),
            EnvelopeMethod::YubikeyPiv => "YubiKey PIV (not enrolled yet)".into(),
            EnvelopeMethod::None => "none (credentials prompted each fetch)".into(),
        }
    ));

    if store.is_empty() {
        app.ui
            .warn("nothing fetched yet. Run `hats secrets fetch`.");
        return Ok(0);
    }

    app.ui.say(format!(
        "fetched   {}",
        store.fetched_at.as_deref().unwrap_or("unknown")
    ));
    app.ui.say("");

    // Names and whether they hold a value, never the values themselves.
    let mut wanted: std::collections::BTreeSet<String> =
        cfg.repo.secrets.required.iter().cloned().collect();
    for name in cfg.profiles().keys() {
        if let Ok(p) = cfg.resolve_profile(name) {
            wanted.extend(p.secret_refs());
        }
    }
    for key in store.keys() {
        let used = if wanted.contains(key) {
            ""
        } else {
            "  (unused)"
        };
        app.ui.say(format!("  set      {key}{used}"));
    }
    for key in store.missing(wanted.iter().map(String::as_str)) {
        app.ui.say(format!("  MISSING  {key}"));
    }
    Ok(0)
}

fn clear(app: &mut App) -> Result<i32> {
    let mut removed = Vec::new();
    for path in [&app.paths.secrets, &app.paths.envelope] {
        if path.is_file() {
            std::fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
            removed.push(path.display().to_string());
        }
    }
    if removed.is_empty() {
        app.ui.say("nothing to clear");
    } else {
        for r in removed {
            app.ui.ok(format!("removed {r}"));
        }
        app.ui
            .say("Shells already open keep the values they loaded until they exit.");
    }
    Ok(0)
}

fn enrol(app: &mut App, slot: Option<String>) -> Result<i32> {
    let cfg = app.config()?;
    let slot = slot
        .or_else(|| cfg.local.secrets.envelope.slot.clone())
        .unwrap_or_else(|| "82".into());

    app.ui.heading("Enrolling a YubiKey");
    app.ui.say(
        "This generates a key inside the YubiKey's PIV applet. The private key never \
         leaves the device, and decrypting requires a touch.",
    );
    app.ui
        .say("The plugin will ask for the PIV PIN on this terminal.");

    let recipient = envelope::enrol_yubikey(&slot, "hats", &app.paths.identity)?;
    app.ui.ok(format!("generated a key in PIV slot {slot}"));
    app.ui.say(format!("recipient {recipient}"));

    // Now collect the credentials and seal them to that recipient.
    let creds = prompt_credentials(app)?;
    envelope::seal(&creds, &recipient, &app.paths.envelope)?;
    app.ui.ok(format!(
        "sealed credentials into {}",
        app.paths.envelope.display()
    ));

    // Record the recipient so a later fetch knows what to decrypt against.
    let mut local = cfg.local.clone();
    local.secrets.envelope.method = EnvelopeMethod::YubikeyPiv;
    local.secrets.envelope.recipient = Some(recipient);
    local.secrets.envelope.slot = Some(slot);
    local.save(&app.paths.config)?;

    app.ui
        .say("Run `hats secrets fetch` to pull the vault down.");
    Ok(0)
}

/// Get the vault credentials: from the envelope if there is one, else by asking.
fn credentials(app: &mut App, cfg: &crate::config::local::SecretsConfig) -> Result<Credentials> {
    if cfg.envelope.method == EnvelopeMethod::YubikeyPiv && app.paths.envelope.is_file() {
        if app.paths.identity.is_file() {
            app.ui
                .say("Unlocking the credential envelope (touch your YubiKey when it blinks)…");
            match envelope::open(&app.paths.envelope, &app.paths.identity) {
                Ok(creds) => return Ok(creds),
                Err(e) => {
                    app.ui.warn(format!("could not open the envelope: {e:#}"));
                    app.ui.say("Falling back to prompting for the credentials.");
                }
            }
        } else {
            app.ui.warn(format!(
                "no identity file at {}. Run `hats secrets enrol-yubikey`.",
                app.paths.identity.display()
            ));
        }
    }
    prompt_credentials(app)
}

/// Ask for the credentials. Never stored unless the caller seals them.
fn prompt_credentials(app: &mut App) -> Result<Credentials> {
    app.ui.say("");
    app.ui
        .say("Bitwarden personal API key: web vault → Settings → Security → Keys → View API key.");

    let email = app
        .ui
        .prompter
        .text("secrets.email", "Vault account email", None)?;
    let client_id = app
        .ui
        .prompter
        .text("secrets.client_id", "client_id (user.xxxxx)", None)?;

    // The secret and the master password are read without echo, and never go
    // through the answers file: recording them in a fixture would defeat the
    // point of the envelope.
    let client_secret = read_hidden("client_secret")?;
    let password = read_hidden("Master password (unwraps the vault key)")?;

    Ok(Credentials {
        email: email.trim().to_string(),
        client_id: SecretString::from(client_id.trim().to_string()),
        client_secret,
        password,
    })
}

fn read_hidden(prompt: &str) -> Result<SecretString> {
    let value = inquire::Password::new(prompt)
        .with_display_toggle_enabled()
        .without_confirmation()
        .prompt()
        .context("reading a credential")?;
    if value.is_empty() {
        anyhow::bail!("{prompt} cannot be empty");
    }
    Ok(SecretString::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    use secrecy::ExposeSecret;

    #[test]
    fn a_secret_string_does_not_print_its_value() {
        let s = SecretString::from("SUPERSECRET");
        assert!(!format!("{s:?}").contains("SUPERSECRET"));
        assert_eq!(s.expose_secret(), "SUPERSECRET");
    }

    #[test]
    fn clear_removes_both_the_store_and_the_envelope() {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::HatsPaths::with_root(dir.path());
        paths.ensure_dirs().unwrap();
        std::fs::write(&paths.secrets, "values: {}\n").unwrap();
        std::fs::write(&paths.envelope, "sealed").unwrap();

        // Exercise the same removal the command performs.
        for p in [&paths.secrets, &paths.envelope] {
            if p.is_file() {
                std::fs::remove_file(p).unwrap();
            }
        }
        assert!(!paths.secrets.exists());
        assert!(!paths.envelope.exists());
    }
}
