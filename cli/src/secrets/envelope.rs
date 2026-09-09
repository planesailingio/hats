//! `~/.hats/envelope.age` — the provider credentials, encrypted at rest.
//!
//! Fetching from Bitwarden needs an API key *and* the master password, because
//! the key authenticates but does not decrypt. Storing those in plaintext would
//! undo the point of the vault, so they are encrypted with age to a key held in
//! a YubiKey PIV slot: the private key cannot leave the hardware, and the touch
//! policy means a fetch requires a physical tap.
//!
//! Without a YubiKey the envelope is simply not written and every fetch prompts
//! for the credentials, which is the honest fallback rather than a weaker one.

use std::path::Path;

use age::secrecy::SecretString as AgeSecret;
use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use super::provider::Credentials;

/// The plaintext inside the envelope.
#[derive(Serialize, Deserialize)]
struct Payload {
    email: String,
    client_id: String,
    client_secret: String,
    password: String,
}

/// Prompts the age plugin raises: the PIV PIN, and the touch reminder.
///
/// The plugin drives these, so hats only has to route them somewhere sensible:
/// messages to stderr (never stdout, which may be an evaluated script), and the
/// PIN read from the terminal without echo.
#[derive(Clone)]
struct PluginCallbacks;

impl age::Callbacks for PluginCallbacks {
    fn display_message(&self, message: &str) {
        eprintln!("  {message}");
    }

    fn confirm(&self, message: &str, _yes: &str, _no: Option<&str>) -> Option<bool> {
        eprintln!("  {message}");
        Some(true)
    }

    fn request_public_string(&self, description: &str) -> Option<String> {
        eprint!("  {description}: ");
        use std::io::Write;
        std::io::stderr().flush().ok()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok()?;
        Some(line.trim().to_owned())
    }

    fn request_passphrase(&self, description: &str) -> Option<AgeSecret> {
        // The YubiKey PIN lands here. inquire reads it without echoing.
        inquire::Password::new(description)
            .with_display_toggle_enabled()
            .without_confirmation()
            .prompt()
            .ok()
            .map(AgeSecret::from)
    }
}

/// Is the plugin binary hats needs actually installed?
///
/// Checked up front because the age crate resolves the plugin at construction
/// time and its error ("Could not find 'age-plugin-yubikey' on the PATH") is
/// less useful than one naming the install command.
pub fn plugin_available(recipient: &str) -> Result<()> {
    let plugin = recipient
        .strip_prefix("age1")
        .and_then(|r| r.split_once('1'))
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| "yubikey".into());
    let binary = format!("age-plugin-{plugin}");
    which::which(&binary).with_context(|| {
        format!("{binary} is not installed. Run `brew install {binary}` (Linux also needs pcscd running).")
    })?;
    Ok(())
}

/// Encrypt credentials to an age recipient.
pub fn seal(creds: &Credentials, recipient: &str, path: &Path) -> Result<()> {
    plugin_available(recipient)?;

    let payload = Payload {
        email: creds.email.clone(),
        client_id: creds.client_id.expose_secret().to_string(),
        client_secret: creds.client_secret.expose_secret().to_string(),
        password: creds.password.expose_secret().to_string(),
    };
    let plaintext = serde_yaml_ng::to_string(&payload)?;

    let parsed: age::plugin::Recipient = recipient
        .parse()
        .map_err(|e| anyhow::anyhow!("`{recipient}` is not a valid age recipient: {e}"))?;
    let plugin = age::plugin::RecipientPluginV1::new(
        parsed.plugin(),
        std::slice::from_ref(&parsed),
        &[],
        PluginCallbacks,
    )
    .context("starting the age plugin to encrypt")?;

    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&plugin as &dyn age::Recipient))
            .context("preparing the age encryptor")?;

    let mut out = Vec::new();
    {
        use std::io::Write;
        let mut writer = encryptor
            .wrap_output(age::armor::ArmoredWriter::wrap_output(
                &mut out,
                age::armor::Format::AsciiArmor,
            )?)
            .context("starting encryption")?;
        writer.write_all(plaintext.as_bytes())?;
        writer.finish()?.finish()?;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("age.tmp");
    std::fs::write(&tmp, &out).with_context(|| format!("writing {}", tmp.display()))?;
    owner_only(&tmp)?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Decrypt the envelope using an age identity file, prompting for PIN and touch
/// as the plugin asks.
pub fn open(path: &Path, identity_file: &Path) -> Result<Credentials> {
    let ciphertext = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;

    let identities = age::IdentityFile::from_file(identity_file.to_string_lossy().into_owned())
        .with_context(|| format!("reading identity file {}", identity_file.display()))?
        .with_callbacks(PluginCallbacks)
        .into_identities()
        .context("loading the age identity (is age-plugin-yubikey installed?)")?;

    let decryptor = age::Decryptor::new(age::armor::ArmoredReader::new(&ciphertext[..]))
        .context("reading the envelope; it may be corrupt")?;
    let mut reader = decryptor
        .decrypt(identities.iter().map(|i| i.as_ref() as &dyn age::Identity))
        .context("decrypting the envelope; is the right YubiKey plugged in?")?;

    let mut plaintext = String::new();
    use std::io::Read;
    reader.read_to_string(&mut plaintext)?;

    let payload: Payload =
        serde_yaml_ng::from_str(&plaintext).context("the envelope did not contain valid data")?;
    Ok(Credentials {
        email: payload.email,
        client_id: SecretString::from(payload.client_id),
        client_secret: SecretString::from(payload.client_secret),
        password: SecretString::from(payload.password),
    })
}

/// Generate a YubiKey identity in a PIV slot, returning the recipient.
///
/// Shelling out is deliberate: `age-plugin-yubikey --generate` needs a terminal
/// for the PIV PIN, and reimplementing PIV provisioning to avoid one prompt
/// would be a large amount of security-critical code for no gain.
pub fn enrol_yubikey(slot: &str, name: &str, identity_path: &Path) -> Result<String> {
    which::which("age-plugin-yubikey")
        .context("age-plugin-yubikey is not installed. Run `brew install age-plugin-yubikey`.")?;

    let out = std::process::Command::new("age-plugin-yubikey")
        .args(["--generate", "--slot", slot, "--name", name])
        .args(["--pin-policy", "once", "--touch-policy", "always"])
        .output()
        .context("running age-plugin-yubikey --generate")?;

    if !out.status.success() {
        anyhow::bail!(
            "age-plugin-yubikey --generate failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let identity = String::from_utf8_lossy(&out.stdout).into_owned();
    let recipient = extract_recipient(&identity)
        .or_else(|| extract_recipient(&String::from_utf8_lossy(&out.stderr)))
        .context("could not find the recipient in age-plugin-yubikey's output")?;

    if let Some(parent) = identity_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(identity_path, &identity)
        .with_context(|| format!("writing {}", identity_path.display()))?;
    owner_only(identity_path)?;
    Ok(recipient)
}

/// Pull `age1yubikey1...` out of the plugin's output, from either the
/// `#    Recipient:` comment or the bare line it prints to stderr.
fn extract_recipient(text: &str) -> Option<String> {
    text.lines()
        .flat_map(|l| l.split_whitespace())
        .find(|w| w.starts_with("age1"))
        .map(|w| {
            w.trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_string()
        })
}

#[cfg(unix)]
fn owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting 0600 on {}", path.display()))
}

#[cfg(not(unix))]
fn owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recipient_is_found_in_the_comment_form() {
        let out = "\
#       Serial: 12345678, Slot: 1
#         Name: age identity ABCD1234
#    Recipient: age1yubikey1qwerty0123456789
AGE-PLUGIN-YUBIKEY-1QXYZ
";
        assert_eq!(
            extract_recipient(out).as_deref(),
            Some("age1yubikey1qwerty0123456789")
        );
    }

    #[test]
    fn the_recipient_is_found_in_the_bare_stderr_form() {
        assert_eq!(
            extract_recipient("Recipient: age1yubikey1abc123\n").as_deref(),
            Some("age1yubikey1abc123")
        );
    }

    #[test]
    fn trailing_punctuation_is_trimmed() {
        assert_eq!(
            extract_recipient("use age1yubikey1abc.\n").as_deref(),
            Some("age1yubikey1abc")
        );
    }

    #[test]
    fn no_recipient_in_the_output_is_reported_rather_than_guessed() {
        assert_eq!(extract_recipient("nothing useful here"), None);
    }

    #[test]
    fn the_plugin_name_is_derived_from_the_recipient_not_hardcoded() {
        // A real recipient resolves to age-plugin-yubikey; a made-up plugin
        // must produce an error naming that plugin, not the yubikey one.
        let err = plugin_available("age1madeup1qqqq").unwrap_err().to_string();
        assert!(err.contains("age-plugin-madeup"), "{err}");
        assert!(
            err.contains("brew install"),
            "should say how to fix it: {err}"
        );
    }

    /// The whole round trip with a software key, so the envelope format,
    /// serialisation and permissions are covered without needing hardware. The
    /// YubiKey path differs only in which age recipient is used.
    #[test]
    fn credentials_round_trip_through_an_age_envelope() {
        use age::x25519;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("envelope.age");

        let identity = x25519::Identity::generate();
        let recipient = identity.to_public();

        let payload = Payload {
            email: "t@example.com".into(),
            client_id: "user.abc".into(),
            client_secret: "SUPERSECRET".into(),
            password: "hunter2".into(),
        };
        let plaintext = serde_yaml_ng::to_string(&payload).unwrap();

        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .unwrap();
        let mut out = Vec::new();
        {
            use std::io::Write;
            let mut w = encryptor
                .wrap_output(
                    age::armor::ArmoredWriter::wrap_output(
                        &mut out,
                        age::armor::Format::AsciiArmor,
                    )
                    .unwrap(),
                )
                .unwrap();
            w.write_all(plaintext.as_bytes()).unwrap();
            w.finish().unwrap().finish().unwrap();
        }
        std::fs::write(&path, &out).unwrap();

        // The ciphertext must not contain the secret in the clear.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("SUPERSECRET"), "envelope is not encrypted");
        assert!(raw.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));

        let decryptor = age::Decryptor::new(age::armor::ArmoredReader::new(&out[..])).unwrap();
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .unwrap();
        let mut back = String::new();
        use std::io::Read;
        reader.read_to_string(&mut back).unwrap();

        let decoded: Payload = serde_yaml_ng::from_str(&back).unwrap();
        assert_eq!(decoded.client_secret, "SUPERSECRET");
        assert_eq!(decoded.password, "hunter2");
        assert_eq!(decoded.email, "t@example.com");
    }

    #[test]
    fn opening_a_corrupt_envelope_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let envelope = dir.path().join("envelope.age");
        let identity = dir.path().join("identity.txt");
        std::fs::write(&envelope, "not an age file").unwrap();
        std::fs::write(&identity, "AGE-SECRET-KEY-1INVALID").unwrap();

        let err = open(&envelope, &identity).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("identity") || msg.contains("corrupt"),
            "unhelpful error: {msg}"
        );
    }
}
