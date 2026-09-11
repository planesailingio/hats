#!/bin/sh
# Creates the machine-local SSH layout that ~/.ssh/config includes, and writes the
# README that explains it. Nothing here is tracked in the repo: client hosts, keys and
# known_hosts stay on this machine only. Re-runs when this script changes so the README
# stays current; the README is hats' own text, and nothing else here is ever touched.
set -eu

ssh_dir="$HOME/.ssh"
conf_d="$ssh_dir/config.d"
kh_d="$ssh_dir/known_hosts.d"

mkdir -p "$conf_d" "$kh_d"
chmod 700 "$ssh_dir" "$conf_d" "$kh_d"

readme="$conf_d/README"
cat > "$readme" <<'README'
~/.ssh/config.d — machine-local SSH config (not in the dotfiles repo)

~/.ssh/config reads three layers, in this order:

    ~/.ssh/config.d/${HATS_HAT}.conf    the active hat's hosts and keys
    ~/.ssh/config.d/common.conf         hosts every hat shares
    Host * defaults                     managed by hats, in ~/.ssh/config

ssh takes the FIRST value it finds for each option, so a hat's file overrides
common.conf, and both override the defaults. Each file is named after its hat:

    ~/.ssh/config.d/acme.conf       read under `hat acme`
    ~/.ssh/config.d/globex.conf     read under `hat globex`
    ~/.ssh/config.d/normal.conf     read under `hat normal`

`hats apply` creates common.conf and a file for every hat in ~/.hats/config.yaml,
each holding one comment line. From then on they are yours: hats never updates or
removes them, even when the hat goes.

A hat with no file (one added since the last apply) just gets common.conf and the
defaults. Nothing else here is read, and hat inheritance does not reach ssh: a hat
that `inherits: normal` does not read normal.conf.

Each file is ordinary ssh config. Hostnames shared across clients (github.com is the
usual one) need nothing special: each hat's file has its own block, and only one of
them is read.

    Host github.com
      IdentityFile ~/.ssh/id_ed25519_acme
      UserKnownHostsFile ~/.ssh/known_hosts.d/acme

    Host bastion.acme.example
      User jane
      IdentityFile ~/.ssh/id_ed25519_acme
      UserKnownHostsFile ~/.ssh/known_hosts.d/acme

Keys: generate per client and keep them here too — never in git:

    ssh-keygen -t ed25519 -C "jane.doe@acme.com" -f ~/.ssh/id_ed25519_acme

Check which key a host will use under a given hat:

    hat acme && ssh -G github.com | grep -i identityfile

Files must be 0600 (`chmod 600 ~/.ssh/config.d/*`) or ssh refuses to read them.

Keep IdentityFile out of common.conf for any host a hat's file also covers, and never
put one under `Host *`: ssh APPENDS IdentityFile entries rather than taking the first,
so the extra key is offered as a fallback and github.com could quietly log you in as
the wrong account. Leave it unset and ssh uses ~/.ssh/id_ed25519 only for hosts
nothing else matched.

Variables in Include need OpenSSH 9.9 or later; `hats doctor` checks. A process with
no hat at all (a GUI app, launchd) makes ssh warn that HATS_HAT has no value, then
carry on with common.conf and the defaults.
README
chmod 600 "$readme"
echo "==> wrote $readme"
