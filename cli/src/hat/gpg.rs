//! Per-shell GnuPG isolation.
//!
//! GnuPG keeps keyrings, ownertrust and agent configuration in `~/.gnupg`, and
//! gpg-agent caches unlocked keys against a socket inside that directory. One
//! shared home means one set of keys and one agent: `gpg --import` or
//! `--edit-key` in one terminal reaches every other, the default key is
//! whichever the keyring says, and a passphrase typed for a client's key stays
//! cached for every shell on the machine. Each hat gets its own `GNUPGHOME`,
//! and with it its own keyring, trustdb and agent.
//!
//! Configuration and *public* material are seeded from the shared directory;
//! secret keys are not. A hat's signing key is imported into the hat, which is
//! the point: `private-keys-v1.d` copied five ways would put every client's
//! key in every client's terminal.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// What a new hat's home is seeded with from the shared one: configuration,
/// the public keyring (both the modern keybox and the legacy ring) and
/// ownertrust. Nothing here is secret, and leaving it behind would mean a hat
/// that cannot verify a signature it could verify yesterday.
const SEEDED: &[&str] = &[
    "gpg.conf",
    "gpg-agent.conf",
    "dirmngr.conf",
    "pubring.kbx",
    "pubring.gpg",
    "trustdb.gpg",
];

/// The longest a unix socket path may be: 104 bytes on macOS and the BSDs,
/// 108 on Linux. The lower of the two, since a config shared between machines
/// should not pass here and fail there.
pub const SOCKET_MAX: usize = 104;

/// The directory a hat's GnuPG home lives in.
fn dir(home: &Path) -> PathBuf {
    home.join(".gnupg").join(".hats")
}

/// Where a hat's own GnuPG home lives.
pub fn config_dir(home: &Path, hat: &str) -> PathBuf {
    dir(home).join(hat)
}

/// Where gpg-agent binds the socket for a hat's home.
///
/// It lives inside `GNUPGHOME`, so a long home plus a long hat name can push
/// it past [`SOCKET_MAX`], and the agent then cannot bind at all. gpg reports
/// that as `No agent running`, naming neither the socket nor the hat, so
/// `hats doctor` measures it instead.
pub fn agent_socket(home: &Path, hat: &str) -> PathBuf {
    config_dir(home, hat).join("S.gpg-agent")
}

/// The shared GnuPG home a per-hat copy is seeded from.
pub fn shared(home: &Path) -> Option<PathBuf> {
    let path = home.join(".gnupg");
    if path.is_dir() { Some(path) } else { None }
}

/// Give this hat its own GnuPG home, seeded once from the shared one if
/// present.
///
/// Returns the path for `$GNUPGHOME`.
pub fn isolate(home: &Path, hat: &str) -> Result<PathBuf> {
    let target = config_dir(home, hat);
    if target.exists() {
        return Ok(target);
    }
    seed(&target, hat, shared(home))?;
    Ok(target)
}

/// Seed this hat's GnuPG home from the shared one if present.
fn seed(target: &Path, hat: &str, shared: Option<PathBuf>) -> Result<()> {
    private_dir(target)?;
    // gpg creates this itself on the first import, but only at whatever the
    // umask allows, and it refuses to use a secret key store that is not
    // owner-only.
    private_dir(&target.join("private-keys-v1.d"))?;

    let mut seeded_conf = false;
    if let Some(shared) = shared {
        for name in SEEDED {
            let from = shared.join(name);
            if from.is_file() {
                std::fs::copy(&from, target.join(name)).with_context(|| {
                    format!("copying {} to {}", from.display(), target.display())
                })?;
                seeded_conf |= *name == "gpg.conf";
            }
        }
    }
    // A directory of keyring files explains nothing about why it exists, so
    // the one file hats writes itself says whose home this is and how to give
    // it a key. Only when the shared home had no gpg.conf to copy.
    if !seeded_conf {
        let path = target.join("gpg.conf");
        std::fs::write(&path, scaffold_text(hat))
            .with_context(|| format!("writing {}", path.display()))?;
    }

    owner_only(target)
}

/// What hats writes into a hat's `gpg.conf` the one time it creates it.
///
/// gpg reads this file, so it has to be valid; a comment block is, and it
/// turns an otherwise opaque directory into something that explains itself.
fn scaffold_text(hat: &str) -> String {
    format!(
        "# GnuPG configuration for hat {hat}, read by shells wearing it. \
         Machine-local;\n\
         # hats created this file and will not touch it again.\n\
         #\n\
         # This directory is the hat's $GNUPGHOME: its own keyring, trustdb and\n\
         # gpg-agent. Give the hat a key from a shell wearing it:\n\
         #\n\
         #   gpg --import acme.key.asc     # or: gpg --full-generate-key\n\
         #\n\
         # then name its id in `identity: {{ signing_key: ... }}` for the hat, and\n\
         # put `[commit] gpgsign = true` in ~/.gitconfig.d/{hat} to sign by default.\n"
    )
}

/// Create `dir` as 0700, parents included. gpg warns about a home anyone else
/// can read and refuses to use a world-readable secret key store.
fn private_dir(dir: &Path) -> Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .with_context(|| format!("creating {}", dir.display()))
}

#[cfg(unix)]
fn owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("setting 0700 on {}", path.display()))
}

#[cfg(not(unix))]
fn owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hat_gets_its_own_gnupg_home() {
        let home = Path::new("/home/t");
        assert_eq!(
            config_dir(home, "acme"),
            PathBuf::from("/home/t/.gnupg/.hats/acme")
        );
        assert_ne!(config_dir(home, "a"), config_dir(home, "b"));
    }

    /// The limit is on the whole path, so it is the home, the `.hats` segment
    /// and the hat's name together that have to fit.
    #[test]
    fn the_agent_socket_sits_inside_the_hats_own_home() {
        let home = Path::new("/Users/jane");
        assert_eq!(
            agent_socket(home, "acme"),
            PathBuf::from("/Users/jane/.gnupg/.hats/acme/S.gpg-agent")
        );
        assert!(agent_socket(home, "acme").as_os_str().len() < SOCKET_MAX);
        // A short home leaves room for the longest name a hat may have, so it
        // takes a deep home as well to run out.
        assert!(agent_socket(home, &"x".repeat(64)).as_os_str().len() < SOCKET_MAX);
        let deep = PathBuf::from(format!("/Users/{}", "d".repeat(60)));
        assert!(agent_socket(&deep, &"x".repeat(32)).as_os_str().len() > SOCKET_MAX);
    }

    #[test]
    fn the_first_use_creates_the_home_and_its_secret_key_store() {
        let home = tempfile::tempdir().unwrap();
        let path = isolate(home.path(), "acme").unwrap();
        assert!(path.is_dir());
        assert_eq!(path, config_dir(home.path(), "acme"));
        assert!(path.join("private-keys-v1.d").is_dir());
    }

    #[test]
    fn two_hats_cannot_see_each_others_keyrings() {
        let home = tempfile::tempdir().unwrap();
        let a = isolate(home.path(), "alpha").unwrap();
        let b = isolate(home.path(), "beta").unwrap();
        assert_ne!(a, b);

        std::fs::write(a.join("pubring.kbx"), "alpha-keys").unwrap();
        assert!(!b.join("pubring.kbx").exists());
    }

    #[test]
    fn seeding_copies_config_and_public_material() {
        let home = tempfile::tempdir().unwrap();
        let shared = home.path().join(".gnupg");
        std::fs::create_dir_all(&shared).unwrap();
        for (name, body) in [
            ("gpg.conf", "shared-conf"),
            ("gpg-agent.conf", "shared-agent"),
            ("pubring.kbx", "shared-pubring"),
            ("trustdb.gpg", "shared-trust"),
        ] {
            std::fs::write(shared.join(name), body).unwrap();
        }

        let path = isolate(home.path(), "seeded").unwrap();
        assert_eq!(
            std::fs::read_to_string(path.join("gpg.conf")).unwrap(),
            "shared-conf",
            "a shared gpg.conf wins over the scaffolded comment"
        );
        assert_eq!(
            std::fs::read_to_string(path.join("pubring.kbx")).unwrap(),
            "shared-pubring"
        );
        assert!(path.join("gpg-agent.conf").is_file());
        assert!(path.join("trustdb.gpg").is_file());
    }

    /// The isolation this whole module exists for: a hat starts with no secret
    /// key, whatever the shared home holds.
    #[test]
    fn secret_keys_are_never_copied_into_a_hat() {
        let home = tempfile::tempdir().unwrap();
        let shared = home.path().join(".gnupg/private-keys-v1.d");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("ABCD.key"), "secret").unwrap();

        let path = isolate(home.path(), "acme").unwrap();
        assert!(
            std::fs::read_dir(path.join("private-keys-v1.d"))
                .unwrap()
                .next()
                .is_none(),
            "the hat's secret key store must start empty"
        );
    }

    #[test]
    fn a_home_with_no_shared_config_gets_a_comment_that_explains_itself() {
        let home = tempfile::tempdir().unwrap();
        let path = isolate(home.path(), "acme").unwrap();
        let conf = std::fs::read_to_string(path.join("gpg.conf")).unwrap();
        assert!(conf.lines().all(|l| l.starts_with('#')), "{conf}");
        assert!(conf.contains("hat acme"), "{conf}");
        assert!(conf.contains("gpg --import"), "{conf}");
    }

    #[test]
    fn seeding_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let path = isolate(home.path(), "acme").unwrap();
        std::fs::write(path.join("gpg.conf"), "edited").unwrap();

        let again = isolate(home.path(), "acme").unwrap();
        assert_eq!(again, path);
        assert_eq!(
            std::fs::read_to_string(again.join("gpg.conf")).unwrap(),
            "edited"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_home_and_the_secret_key_store_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let path = isolate(home.path(), "acme").unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o700, "gpg refuses an open home");
        assert_eq!(mode(&path.join("private-keys-v1.d")), 0o700);
    }
}
