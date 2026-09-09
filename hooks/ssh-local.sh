#!/bin/sh
# Runs ONCE after first apply. Creates the machine-local SSH layout that ~/.ssh/config
# includes. Nothing here is tracked in the repo: client hosts, keys and known_hosts stay
# on this machine only. Idempotent — never overwrites an existing file.
set -eu

ssh_dir="$HOME/.ssh"
conf_d="$ssh_dir/config.d"
kh_d="$ssh_dir/known_hosts.d"

mkdir -p "$conf_d" "$kh_d"
chmod 700 "$ssh_dir" "$conf_d" "$kh_d"

readme="$conf_d/README"
if [ ! -f "$readme" ]; then
  cat > "$readme" <<'README'
~/.ssh/config.d — machine-local SSH config (not in the dotfiles repo)

~/.ssh/config does `Include ~/.ssh/config.d/*.conf`, so drop one file per client here:

    ~/.ssh/config.d/acme.conf
    ~/.ssh/config.d/globex.conf
    ~/.ssh/config.d/normal.conf

Only *.conf files are included; this README and anything else are ignored.

Recipe for a client file — hosts unique to the client are plain Host blocks:

    Host bastion.acme.example
      User jane
      IdentityFile ~/.ssh/id_ed25519_acme
      UserKnownHostsFile ~/.ssh/known_hosts.d/acme

Hostnames shared across clients (github.com is the usual one) are selected by the
active shell profile. `profile <name>` exports DEV_PROFILE, and ssh's `Match exec`
inherits that environment, so each terminal picks its own key with no wrappers:

    Match host github.com exec "test \"$DEV_PROFILE\" = acme"
      IdentityFile ~/.ssh/id_ed25519_acme
      UserKnownHostsFile ~/.ssh/known_hosts.d/acme

Keys: generate per client and keep them here too — never in git:

    ssh-keygen -t ed25519 -C "jane.doe@acme.com" -f ~/.ssh/id_ed25519_acme

Check which key a host will use from a given profile:

    profile acme && ssh -G github.com | grep -i identityfile

Files must be 0600 (`chmod 600 ~/.ssh/config.d/*.conf`) or ssh refuses to read them.

Don't add a catch-all `IdentityFile` under `Host *`: ssh APPENDS IdentityFile entries, so a
default key would be offered as a fallback after the client key and github.com could quietly
log you in as the wrong account. Leave it unset and ssh uses ~/.ssh/id_ed25519 only for
hosts nothing else matched.
README
  chmod 600 "$readme"
  echo "==> seeded $readme"
else
  echo "==> $readme exists; leaving it alone"
fi
