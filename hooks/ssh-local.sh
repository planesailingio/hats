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
each holding one comment line; `hats hat create` does the same for a new hat. From
then on they are yours: hats never updates them. `hats hat delete <hat>` is the one
thing that removes a hat's file, moving it to ~/.hats/backups with the hat.

A hat with no file (one added by hand since the last apply) just gets common.conf
and the defaults. Nothing else here is read, and hat inheritance does not reach ssh: a hat
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

coder: `coder config-ssh` writes to ~/.ssh/config unless told otherwise, and that
file is hats'. Under a hat, hats exports CODER_SSH_CONFIG_FILE pointing at that
hat's file here, so the workspace hosts land in it and resolve under that hat
alone. Run `coder config-ssh` once in each hat that uses coder; coder keeps its
hosts between its own marker comments and leaves the rest of the file alone.

Other tools
-----------

ssh is one of several tools that follow the hat. For each, hats points the tool
at a per-hat file or directory through an environment variable, exported by
`hat <name>` and cleared again on every switch. For a tool hats does not handle,
do the same from the hat's `env:` in ~/.hats/config.yaml. Values are exported
exactly as written, so give absolute paths, not ~ or $HOME:

    hats:
      acme:
        coder: { url: https://coder.acme.example }
        env:
          GH_CONFIG_DIR: /Users/jane/.config/gh/hats/acme
          TF_TOKEN_app_terraform_io: { secret: acme/tfc }

| Tool | Default config location | Per-hat mechanism | Handled by | Notes |
| --- | --- | --- | --- | --- |
| shell env | `env:` and `path:` per hat, in `~/.hats/config.yaml` | exported by `hat <name>` | hats | Anything else. Cleared on every switch; `{ secret: <key> }` values come from `hats secrets fetch`. |
| ssh | `~/.ssh/config` → `config.d/${HATS_HAT}.conf` → `common.conf` | `$HATS_HAT` in `Include` | hats creates, you fill | This directory. |
| git | `~/.gitconfig` + `~/.gitconfig.d/<hat>` | `GIT_AUTHOR_*`, `GIT_COMMITTER_*`, `GIT_CONFIG_*` include | hats | Identity from `identity:`. `url.insteadOf`, signing and credential helpers go in `~/.gitconfig.d/<hat>`. |
| aws | `~/.aws/.hats/<hat>.config`, `<hat>.credentials` | `AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE` | hats, on by default | `aws sso login` writes only this hat's files. `AWS_PROFILE` and `AWS_REGION` are cleared on every switch: set them in `env:`. |
| kubectl | `~/.kube/config.<hat>` | `KUBECONFIG`, `kube: { context }` | hats, on by default | A `use-context` cannot leak between terminals. |
| k9s | `~/.config/k9s/hats/<hat>/` | `K9S_CONFIG_DIR` | hats, on by default | Links back to the shared theme; plugins, aliases and hotkeys per hat. |
| coder | `~/.config/coderv2/hats/<hat>/` | `CODER_CONFIG_DIR`, `CODER_URL` from `coder: { url }`, `CODER_SSH_CONFIG_FILE` | hats, on by default | Own login per hat, token in that directory rather than the Keychain. A stray `CODER_SESSION_TOKEN` is cleared on every switch. |
| helm | `~/Library/Preferences/helm`, `~/Library/Caches/helm` (macOS) | `HELM_REPOSITORY_CONFIG`, `HELM_REGISTRY_CONFIG` | you, via `env:` | Already follows the kube context; split only for private repos or registry logins. |
| gh | `~/.config/gh` | `GH_CONFIG_DIR` (or `GH_TOKEN`, `GH_HOST`) | you, via `env:` | A `gh auth login` per client account. |
| docker | `~/.docker/config.json` | `DOCKER_CONFIG`, `DOCKER_CONTEXT` | you, via `env:` | A new `DOCKER_CONFIG` also hides `cli-plugins/`: link it in. |
| az | `~/.azure` | `AZURE_CONFIG_DIR` | you, via `env:` | Logins and subscriptions per hat, as for AWS. |
| terraform, tofu | `~/.terraformrc` | `TF_CLI_CONFIG_FILE`, `TF_TOKEN_<host>`, `TF_VAR_*` | you, via `env:` | Registry and HCP tokens suit `{ secret: }`. |
| bw | `~/Library/Application Support/Bitwarden CLI` (macOS) | `BITWARDENCLI_APPDATA_DIR` | you, via `env:` | Only for a separate vault account per client. |
| npm | `~/.npmrc` | `NPM_CONFIG_USERCONFIG` | you, via `env:` | Private registry tokens per client. |
| mise, direnv | `mise.toml`, `.envrc` per repo | per directory | the repo | For settings tied to a project rather than a client. |
| starship | `~/.config/starship.toml` | reads `$HATS_HAT` | hats (theme) | Shows the hat, kube context and AWS in the prompt. |
README
chmod 600 "$readme"
echo "==> wrote $readme"
