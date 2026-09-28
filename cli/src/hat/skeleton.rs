//! The base skeleton files hats creates once and then leaves alone.
//!
//! These are the identity-shaped files that used to be managed by the
//! dotfiles engine before it moved out to `bosun`: `~/.gitconfig`,
//! `~/.ssh/config`, `~/.terraformrc` and `~/.tofurc`, plus the machine-local
//! ssh directories and their README. They hold the *profile patterns* — the
//! Include lines and per-hat mechanisms every hat relies on — so hats owns
//! them; the styling that changes over time (delta, aliases, themes) stays
//! with bosun in files git includes from here.
//!
//! Created by `hats init` and `hats hat sync` when missing, never updated:
//! after the first write they are yours.

use std::path::Path;

use super::scaffold::{Kind, Scaffold, tilde};
use crate::config::Config;
use crate::platform::{Os, Platform};

/// Every base skeleton that does not exist yet. Sorted with the rest of the
/// scaffolds by the caller.
pub fn wanted(cfg: &Config, platform: &Platform, signing_key: Option<&str>) -> Vec<Scaffold> {
    let home = &platform.home;
    let mut out = vec![
        text(
            home,
            home.join(".gitconfig"),
            gitconfig_text(cfg, signing_key),
        ),
        text(home, home.join(".terraformrc"), TERRAFORMRC.to_string()),
        text(home, home.join(".tofurc"), TOFURC.to_string()),
        dir(home, home.join(".cache/opentofu/plugin-cache")),
    ];
    if cfg.ssh_enabled() {
        out.push(text(
            home,
            super::ssh::skeleton_path(home),
            ssh_config_text(platform.os),
        ));
        out.push(text(
            home,
            home.join(".ssh/config.d/README"),
            SSH_README.to_string(),
        ));
        out.push(dir(home, home.join(".ssh/known_hosts.d")));
    }
    out
}

fn text(home: &Path, target: std::path::PathBuf, body: String) -> Scaffold {
    Scaffold {
        display: tilde(&target, home),
        target,
        how: "base skeleton".into(),
        kind: Kind::Text(body),
    }
}

fn dir(home: &Path, target: std::path::PathBuf) -> Scaffold {
    Scaffold {
        display: tilde(&target, home),
        target,
        how: "empty, owner-only".into(),
        kind: Kind::Dir,
    }
}

/// `~/.gitconfig`: the fallback identity and the include pattern. Styling
/// (delta, aliases, LFS) lives in bosun's ~/.config/git/style.gitconfig,
/// which git silently skips when bosun has not written it.
fn gitconfig_text(cfg: &Config, signing_key: Option<&str>) -> String {
    let identity = cfg.local.identity.clone().unwrap_or_default();
    let mut out = String::from(
        "# ~/.gitconfig — scaffolded once by hats, yours after that.\n\
         #\n\
         # Identity here is only the fallback for tools that ignore the environment.\n\
         # The active hat sets GIT_AUTHOR_*/GIT_COMMITTER_* per shell, and those win.\n\
         # Each hat's shell also includes ~/.gitconfig.d/<hat> (created by\n\
         # `hats hat sync`), so per-client settings go there rather than here.\n\
         [user]\n",
    );
    out.push_str(&format!(
        "\temail = {}\n\tname = {}\n",
        identity.email.unwrap_or_default(),
        identity.name.unwrap_or_default()
    ));
    if let Some(key) = signing_key.filter(|k| !k.is_empty()) {
        out.push_str(&format!("\tsigningkey = {key}\n"));
    }
    out.push_str(
        "\n# Styling and quality-of-life defaults, managed by bosun. git skips the\n\
         # include when the file is missing, so this works without bosun too.\n\
         [include]\n\tpath = ~/.config/git/style.gitconfig\n",
    );
    out
}

/// `~/.ssh/config`: the three-layer skeleton the per-hat files rely on.
fn ssh_config_text(os: Os) -> String {
    let keychain = match os {
        Os::Darwin => "  UseKeychain yes\n",
        _ => "",
    };
    format!(
        "# ~/.ssh/config — shared skeleton. Scaffolded once by hats, yours after that.\n\
         #\n\
         # Three layers, read top to bottom. ssh takes the FIRST value it finds for each\n\
         # option, so each layer overrides everything below it:\n\
         #\n\
         #   1. ~/.ssh/config.d/<hat>.conf — the active hat's hosts and keys. `hat <name>`\n\
         #      exports $HATS_HAT and ssh expands it here, per process, so plain ssh, scp,\n\
         #      rsync and git all follow the shell's hat with no wrappers. A hat with no\n\
         #      file is fine: it just gets the layers below.\n\
         #   2. ~/.ssh/config.d/common.conf — hosts every hat shares.\n\
         #   3. The `Host *` defaults at the bottom of this file.\n\
         #\n\
         # Both config.d files are machine-local. `hats hat sync` creates each one once,\n\
         # with a comment line, and never touches it again. ~/.ssh/config.d/README has\n\
         # the recipe.\n\
         #\n\
         # Variables in Include need OpenSSH 9.9+ (`hats doctor` checks). A process with no\n\
         # hat at all (a GUI app, launchd) makes ssh warn that HATS_HAT has no value; it\n\
         # skips layer 1 and carries on.\n\
         Include ~/.ssh/config.d/${{HATS_HAT}}.conf\n\
         Include ~/.ssh/config.d/common.conf\n\
         \n\
         Host *\n\
         \x20 AddKeysToAgent yes\n\
         {keychain}\
         \x20 ServerAliveInterval 30\n\
         \x20 ServerAliveCountMax 3\n\
         \x20 HashKnownHosts no\n\
         \x20 IdentitiesOnly yes\n"
    )
}

const TERRAFORMRC: &str = r#"# ~/.terraformrc — Terraform CLI configuration. Scaffolded once by hats,
# yours after that.
#
# OpenTofu reads ~/.tofurc in preference to this file, and hats scaffolds that
# too, with the same settings. Under a hat neither is read: TF_CLI_CONFIG_FILE
# points both tools at ~/.terraform.d/.hats/<hat>.tfrc, a copy of this file
# taken when the hat was created. A later change here does not reach a hat
# that already has its copy; edit that copy as well.

# One shared provider cache, so `init` links a provider already on disk rather
# than downloading a few hundred megabytes into every .terraform/. Terraform
# and OpenTofu share it safely: providers are filed by registry hostname, so
# registry.terraform.io and registry.opentofu.org never collide. Neither tool
# creates the directory; `hats hat sync` does.
#
# Terraform does not lock the cache, so do not run `init` concurrently against
# a cold one (terragrunt run-all is the usual culprit). OpenTofu 1.10 and later
# locks it.
plugin_cache_dir = "$HOME/.cache/opentofu/plugin-cache"

# No anonymous version-check calls to HashiCorp from a client machine. OpenTofu
# has no checkpoint and ignores this.
disable_checkpoint = true

# Deliberately left at their defaults:
#
# plugin_cache_may_break_dependency_lock_file
#   Setting it lets `init` use a cached provider without recording every
#   platform's checksums, which leaves .terraform.lock.hcl incomplete for CI
#   and for colleagues. Run `terraform providers lock -platform=...` instead.
#
# credentials
#   Tokens do not belong here. Give a hat `TF_TOKEN_<host>: { secret: <key> }`
#   in its `env:`, or put a `credentials` block in that hat's own tfrc.
#
# provider_installation
#   The default (direct from the registry, through the cache above) is right
#   unless a client needs a network mirror, and that belongs in the hat's tfrc.
"#;

const TOFURC: &str = r#"# ~/.tofurc — OpenTofu CLI configuration. Scaffolded once by hats, yours
# after that.
#
# OpenTofu reads this file in preference to ~/.terraformrc, which hats
# scaffolds too, with the same settings. Under a hat neither is read:
# TF_CLI_CONFIG_FILE points both tools at ~/.terraform.d/.hats/<hat>.tfrc, a
# copy of ~/.terraformrc taken when the hat was created. A later change here
# does not reach a hat that already has its copy; edit that copy as well.

# One shared provider cache, so `init` links a provider already on disk rather
# than downloading a few hundred megabytes into every .terraform/. OpenTofu
# and Terraform share it safely: providers are filed by registry hostname, so
# registry.opentofu.org and registry.terraform.io never collide. Neither tool
# creates the directory; `hats hat sync` does.
#
# OpenTofu 1.10 and later locks the cache, so concurrent `init` runs are safe.
# Earlier versions do not: avoid terragrunt run-all against a cold cache there.
plugin_cache_dir = "$HOME/.cache/opentofu/plugin-cache"

# Deliberately left at their defaults:
#
# plugin_cache_may_break_dependency_lock_file
#   Setting it lets `init` use a cached provider without recording every
#   platform's checksums, which leaves .terraform.lock.hcl incomplete for CI
#   and for colleagues. Run `tofu providers lock -platform=...` instead.
#
# credentials
#   Tokens do not belong here. Give a hat `TF_TOKEN_<host>: { secret: <key> }`
#   in its `env:`, or put a `credentials` block in that hat's own tfrc.
#
# provider_installation
#   The default (direct from the registry, through the cache above) is right
#   unless a client needs a network mirror, and that belongs in the hat's tfrc.
"#;

const SSH_README: &str = r#"~/.ssh/config.d — machine-local SSH config (not in any repo)

~/.ssh/config reads three layers, in this order:

    ~/.ssh/config.d/${HATS_HAT}.conf    the active hat's hosts and keys
    ~/.ssh/config.d/common.conf         hosts every hat shares
    Host * defaults                     in ~/.ssh/config

ssh takes the FIRST value it finds for each option, so a hat's file overrides
common.conf, and both override the defaults. Each file is named after its hat:

    ~/.ssh/config.d/acme.conf       read under `hat acme`
    ~/.ssh/config.d/normal.conf     read under `hat normal`

`hats hat sync` creates common.conf and a file for every hat in
~/.hats/config.yaml, each holding one comment line; `hats hat create` does the
same for a new hat. From then on they are yours: hats never updates them.
`hats hat delete <hat>` is the one thing that removes a hat's file, moving it
to ~/.hats/backups with the hat.

A hat with no file just gets common.conf and the defaults. Hat inheritance
does not reach ssh: a hat that `inherits: normal` does not read normal.conf.

Each file is ordinary ssh config. Hostnames shared across clients (github.com
is the usual one) need nothing special: each hat's file has its own block, and
only one of them is read.

    Host github.com
      IdentityFile ~/.ssh/id_ed25519_acme
      UserKnownHostsFile ~/.ssh/known_hosts.d/acme

Keys: generate per client and keep them here too — never in git:

    ssh-keygen -t ed25519 -C "jane.doe@acme.com" -f ~/.ssh/id_ed25519_acme

Check which key a host will use under a given hat:

    hat acme && ssh -G github.com | grep -i identityfile

Files must be 0600 (`chmod 600 ~/.ssh/config.d/*`) or ssh refuses to read them.

Keep IdentityFile out of common.conf for any host a hat's file also covers, and
never put one under `Host *`: ssh APPENDS IdentityFile entries rather than
taking the first, so the extra key is offered as a fallback and github.com
could quietly log you in as the wrong account.

Variables in Include need OpenSSH 9.9 or later; `hats doctor` checks. A process
with no hat at all (a GUI app, launchd) makes ssh warn that HATS_HAT has no
value, then carry on with common.conf and the defaults.

coder: `coder config-ssh` writes to ~/.ssh/config unless told otherwise. Under
a hat, hats exports CODER_SSH_CONFIG_FILE pointing at that hat's file here, so
the workspace hosts land in it and resolve under that hat alone.
"#;

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use std::path::PathBuf;

    fn platform(home: &Path, os: Os) -> Platform {
        Platform {
            os,
            arch: crate::platform::Arch::Arm64,
            home: home.to_path_buf(),
            user: "t".into(),
            hostname: "h".into(),
            brew_prefix: PathBuf::from("/opt/homebrew"),
        }
    }

    #[test]
    fn the_base_set_covers_git_ssh_and_terraform() {
        let home = tempfile::tempdir().unwrap();
        let got: Vec<String> = wanted(&config(PROFILES), &platform(home.path(), Os::Darwin), None)
            .into_iter()
            .map(|s| s.display)
            .collect();
        for want in [
            "~/.gitconfig",
            "~/.ssh/config",
            "~/.ssh/config.d/README",
            "~/.ssh/known_hosts.d",
            "~/.terraformrc",
            "~/.tofurc",
            "~/.cache/opentofu/plugin-cache",
        ] {
            assert!(got.contains(&want.to_string()), "missing {want}: {got:?}");
        }
    }

    #[test]
    fn ssh_skeletons_follow_the_ssh_feature() {
        let home = tempfile::tempdir().unwrap();
        let cfg = config(&format!("features: {{ ssh: false }}\n{PROFILES}"));
        let got: Vec<String> = wanted(&cfg, &platform(home.path(), Os::Darwin), None)
            .into_iter()
            .map(|s| s.display)
            .collect();
        assert!(!got.iter().any(|d| d.contains(".ssh")), "{got:?}");
        assert!(got.contains(&"~/.gitconfig".to_string()));
    }

    #[test]
    fn the_gitconfig_carries_identity_signing_key_and_the_style_include() {
        let cfg =
            config("identity: { name: Jane, email: jane@example.com }\nhats:\n  normal: {}\n");
        let text = gitconfig_text(&cfg, Some("ABC123"));
        assert!(text.contains("email = jane@example.com"), "{text}");
        assert!(text.contains("name = Jane"), "{text}");
        assert!(text.contains("signingkey = ABC123"), "{text}");
        assert!(
            text.contains("path = ~/.config/git/style.gitconfig"),
            "{text}"
        );

        let bare = gitconfig_text(&cfg, None);
        assert!(!bare.contains("signingkey"), "{bare}");
    }

    #[test]
    fn use_keychain_is_darwin_only() {
        assert!(ssh_config_text(Os::Darwin).contains("UseKeychain yes"));
        assert!(!ssh_config_text(Os::Linux).contains("UseKeychain"));
        assert!(
            ssh_config_text(Os::Linux).contains("Include ~/.ssh/config.d/${HATS_HAT}.conf"),
            "the literal include must survive the format string"
        );
    }
}
