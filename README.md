<div align="center">

# 🎩 hats

**Per-terminal identity switching for macOS and Linux.**

[![CI](https://github.com/planesailingio/hats/actions/workflows/ci.yml/badge.svg)](https://github.com/planesailingio/hats/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/tag/planesailingio/hats?label=release&sort=semver)](https://github.com/planesailingio/hats/releases)
[![Licence](https://img.shields.io/github/license/planesailingio/hats)](LICENSE)
![macOS & Linux](https://img.shields.io/badge/macOS%20%C2%B7%20Linux-supported-informational)

</div>

---

## Overview

I wear a lot of hats: several customers and products, each with its own
environments and identities. Moving between them meant exporting environment
variables, symlinking or rendering config files, running helper scripts, or
starting a container just to work as the right identity. That time should have
gone on the actual work. Over the years I tried solving it with bash aliases
and built up a collection of bash functions to help, but there had to be a
better way. `hats` is that better way.

With hats you can:

- Switch your shell identity
  - git identity, SSH configs, cloud credentials, kube context, tokens
  and environment variables together with one command, `hat <name>`, in the
  current terminal only.
- Keep contexts isolated. Each hat gets its own copy of shared tool config, and
  the previous hat's variables are unset on every switch, so commands such as
  `kubectl config use-context` or `aws sso login` in one terminal never affect
  another.

It ships as a single binary with no runtime dependencies.

> Think of hats as the chezmoi for environment variables.

hats used to carry a whole dotfiles engine too. That half is now
[bosun](https://github.com/planesailingio/bosun), a sibling tool that
bootstraps the machine itself: Homebrew bundles, zsh, starship, themes and
macOS defaults. The two are independent — hats works without bosun and bosun
without hats — and meet at three small seams: bosun's zsh config loads
`hats shell-init zsh` behind a guard, its starship prompt shows `$HATS_HAT`,
and the `~/.gitconfig` hats scaffolds includes bosun's git styling fragment.

## Supported Tools

Most CLI tools store their active context in a file under your home directory, so
changing it in one terminal changes it in all of them:

| Tool         | Where the active context is stored normally    | Hats equivalent location                            | Selected by                                                   |
| ------------ | ---------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------------- |
| git          | `~/.gitconfig`                                 | `~/.gitconfig.d/<hat>`                              | `GIT_CONFIG_KEY_0`, plus `GIT_AUTHOR_*` and `GIT_COMMITTER_*` |
| kubectl      | `~/.kube/config`                               | `~/.kube/config.<hat>`                              | `KUBECONFIG`                                                  |
| AWS CLI      | `~/.aws/config` and `~/.aws/credentials`       | `~/.aws/.hats/<hat>.config` and `<hat>.credentials` | `AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE`              |
| Terraform    | `~/.terraformrc` and `~/.terraform.d/`         | `~/.terraform.d/.hats/<hat>.tfrc`                   | `TF_CLI_CONFIG_FILE`                                          |
| Azure CLI    | `~/.azure/`                                    | `~/.azure/.hats/<hat>/`                             | `AZURE_CONFIG_DIR`                                            |
| GitHub CLI   | `~/.config/gh/`                                | `~/.config/gh/.hats/<hat>/`                         | `GH_CONFIG_DIR`                                               |
| VS Code      | one global `settings.json`                     | a VS Code profile per hat                           | `code --profile <hat>`, then VS Code remembers the folder     |
| npm, pip     | `~/.npmrc`, a configured index URL             | the hat's `env:`                                    | `NPM_CONFIG_USERCONFIG`, `PIP_INDEX_URL`                      |
| node, python | whatever the version manager last made default | the hat's `path:`                                   | `PATH`, prepended in that shell alone                         |
| API tokens   | wherever they were last exported               | `~/.hats/secrets.yaml`                              | the hat's `env:` keys, unset on the next `hat`                |

This causes multiple issues including:
1. Git commits with the wrong identity
2. Switching context means updating several tools by hand, and a missed step
   produces no error.
3. Tokens, registry URLs and region variables from the previous context stay
   set until something unsets them.

Existing tools each cover part of this. git's `includeIf gitdir:` handles git
identity, based on repository location. direnv sets variables based on the
current directory. kubie isolates kube contexts. None of them covers every tool,
and using several together means several mechanisms that can disagree.

hats applies the whole context with one command and shows the active hat in
your prompt. Switching in one terminal does not affect any other. A folder can
also name its hat in a `.hat` file, and the shell then switches on `cd` (see
[Per-folder hats](#per-folder-hats)).

## Quick start

### 1. Install

```sh
brew install planesailingio/tools/hats
```

On a new Mac, use bosun's bootstrap instead: it installs Xcode command line
tools, Homebrew, bosun and hats, then runs both wizards:

```sh
sh -c "$(curl -fsLS https://raw.githubusercontent.com/planesailingio/bosun/main/bootstrap.sh)"
```

### 2. Initialise

```sh
hats init
```

This asks who you are, which features you want (per-hat ssh, VS Code
profiles), and prompts for one or more hats: git name and email, kube context
and terminal tint. Additional hats can inherit from an existing one. It then
scaffolds the base skeletons (`~/.gitconfig`, `~/.ssh/config`,
`~/.terraformrc`, `~/.tofurc`) and every hat's per-tool files — each created
once, never touched again. Nothing needs the network.

### 3. Switch hat

Open a new terminal (bosun's zsh config loads the `hat` function; without
bosun, add `eval "$(hats shell-init zsh)"` to your `.zshrc`), then:

```sh
hat          # interactive picker
hat acme     # or switch directly
```

If you configured a secrets backend (experimental) during `hats init`, run
`hats secrets fetch` to download your tokens. `hats doctor` reports anything
missing from the machine, and `hats hat sync` recreates any scaffold that has
gone missing.

## Example: a hat's lifecycle

### Create

`hats hat create` adds the hat to your config and creates its per-tool files:

```console
$ hats hat create acme --git-email jane.doe@acme.com
> Inherit defaults from `normal`? Yes
> [acme] git name Jane
> [acme] terminal tint #331420
✓ added hat `acme` to ~/.hats/config.yaml
✓ created ~/.aws/.hats/acme.config  (copy of ~/.aws/config)
✓ created ~/.aws/.hats/acme.credentials  (empty)
✓ created ~/.config/k9s/hats/acme/config.yaml  (→ ~/.config/k9s/config.yaml)
✓ created ~/.gitconfig.d/acme  (comment line)
✓ created ~/.kube/config.acme  (copy of ~/.kube/config)
✓ created ~/.ssh/config.d/acme.conf  (comment line)

Put it on with `hat acme`.
```

To give the hat its own SSH key, generate one and reference it from
`~/.ssh/config.d/acme.conf`:

```ssh-config
Host *
  IdentityFile ~/.ssh/acme.id_ed25519
  IdentitiesOnly yes
```

### Use

In the terminal where the hat is active, git, SSH and kubectl all use the hat's
settings:

```console
$ hat acme
⛭ hat: acme  (git=jane.doe@acme.com  kube=acme)

$ git var GIT_AUTHOR_IDENT
Jane <jane.doe@acme.com> 1789132800 +0100

$ ssh -G github.com | grep identityfile
identityfile /Users/jane/.ssh/acme.id_ed25519

$ echo $KUBECONFIG
/Users/jane/.kube/config.acme

$ kubectl config use-context acme-staging
Switched to context "acme-staging".
```

A second terminal that was already open keeps its own hat, including its kube
context:

```console
$ hats hat current --summary
⛭ hat: normal  (git=jane@example.com  kube=-)

$ ssh -G github.com | grep identityfile
identityfile /Users/jane/.ssh/id_ed25519

$ echo $KUBECONFIG
/Users/jane/.kube/config.normal
```

### Delete

`hats hat delete` removes the hat from your config and moves all of its files,
including any edits you made, to a timestamped backup directory:

```console
$ hats hat delete acme
Deleting hat `acme` removes it from ~/.hats/config.yaml and moves its files to ~/.hats/backups:
  - ~/.aws/.hats/acme.config
  - ~/.aws/.hats/acme.credentials
  - ~/.config/k9s/hats/acme
  - ~/.gitconfig.d/acme
  - ~/.kube/config.acme
  - ~/.ssh/config.d/acme.conf
> Delete hat `acme`? Yes
✓ moved ~/.aws/.hats/acme.config
✓ moved ~/.aws/.hats/acme.credentials
✓ moved ~/.config/k9s/hats/acme
✓ moved ~/.gitconfig.d/acme
✓ moved ~/.kube/config.acme
✓ moved ~/.ssh/config.d/acme.conf
✓ removed hat `acme` from ~/.hats/config.yaml
Undo by copying back from /Users/jane/.hats/backups/20261211T170412Z
```

No credentials, kubeconfigs or SSH host entries for the hat remain in place.

## Day-to-day use

The most frequently used commands:

```sh
hat                      # switch this terminal (picker, or `hat <name>`)
hats hat sync            # recreate any missing scaffold or profile
hats secrets fetch       # refresh tokens from the vault
```

Each terminal has its own hat. Opening a new terminal for a different context
means running `hat` in that terminal, unless the folder it opens in names a hat
(see [Per-folder hats](#per-folder-hats)).

### Changing a hat

To change a hat's settings, such as a token or AWS account, edit its entry
under `hats:` in `~/.hats/config.yaml`, then run `hat <name>` again in any shell
that needs the change. Nothing else is needed, because each switch reads the
config when it runs. The exception is the top-level `identity:` block, which
seeded `~/.gitconfig` when it was scaffolded; the file is yours now, so edit
it directly.

### Adding a token

Add the token to your vault (see [Secrets](#secrets)), run `hats secrets fetch`,
and reference it from a hat as `{ secret: <key> }`.

### Per-folder hats

A folder can name the hat its whole tree wears. Put the hat's name in a file
called `.hat`:

```sh
echo acme > ~/work/acme/.hat
```

From then on, `cd` into `~/work/acme` or anywhere below it puts the acme hat
on, and a terminal that opens there starts in it. The rules:

- The nearest `.hat` at or above the current directory wins, so a subfolder can
  name a different hat from its parent.
- Leaving the tree puts back the hat that was on before the folder took over.
- A switch happens only when the `.hat` file in effect changes. If you run
  `hat other` by hand inside a tree, that choice stays until you leave the tree
  or cross into another one.
- The file holds one hat name. Blank lines and lines starting with `#` are
  ignored. An edit to a `.hat` already in effect is picked up the next time the
  shell enters the tree, or in a new terminal.
- A `.hat` that names a hat this machine does not have switches nothing, and
  prints a warning on each `cd` within that tree until it is fixed.

The file contains no code, so there is nothing to approve as there is with
direnv. A `.hat` in a repository you cloned can only select one of your own
hats by name, and a hat that changes on `cd` is announced with the same summary
line `hat` prints.

The shell tracks this with two variables: `HATS_HAT_FILE`, the `.hat` file in
effect, and `HATS_HAT_PREV`, the hat to go back to. `hats env --here` prints
what the `cd` hook would run in the current directory, and prints nothing when
there is nothing to change.

### The `hat` function

`hat` is a shell function, not a `hats` subcommand, because a child process
cannot modify its parent shell's environment. `hats shell-init zsh` defines the
function and is loaded from your `.zshrc`, directly or via a file it sources
(bosun's managed zsh config carries the line already). `hats env <name>`
prints the shell code a switch would run without executing it. If `hat` is not
found, the shell started before the line was in place; open a new one.

## Prompt

Switching prints a one-line summary:

```console
$ hat acme
⛭ hat: acme  (git=jane.doe@acme.com  kube=acme)
```

With bosun's starship config, the prompt then shows the active hat and kube
context, coloured by environment:

```console
╭─jane@laptop ~/git/acme/platform ‹main ✔›  󱃖 acme  ☸ staging  acme-aws
╰─➤ terraform apply

╭─jane@laptop ~/git/acme/platform ‹main ✔›  󱃖 acme  🚨 ☸ prod-eu-west-1  acme-aws
╰─➤ ▏
```

Development contexts are green, staging yellow, and production bright red with
a 🚨 marker. Any prompt can do the same: the hat is just `$HATS_HAT`.

## Configuring hats

Hats are defined in `~/.hats/config.yaml`, which is machine-local and not kept
in git:

```yaml
features:
  ssh: true        # per-hat ~/.ssh/config.d/<hat>.conf
  vscode: false    # per-hat VS Code profiles and the `code` wrapper
hats:
  normal:
    colour: "#2a2040"
    identity: { name: Jane, email: jane@example.com }

  acme:
    inherits: normal
    colour: "#331420"
    identity: { email: jane.doe@acme.com }
    kube: { context: acme }
    env:
      JIRA_TOKEN: { secret: acme/jira }
```

- `inherits` merges in the parent hat, so a child only needs to specify what
  differs.
- `{ secret: ... }` is a reference to a fetched secret. The value itself is
  never stored in this file.

### Base skeletons

`hats init` (and `hats hat sync` after it) scaffolds the shared files the
per-hat mechanisms hang off, each created once and then yours:

- `~/.gitconfig` — the fallback identity, plus an `[include]` of bosun's
  styling fragment (`~/.config/git/style.gitconfig`), which git skips when
  bosun is not installed.
- `~/.ssh/config` — the three-layer Include skeleton, plus `~/.ssh/config.d/`
  with its README and `~/.ssh/known_hosts.d/` (with `features.ssh`).
- `~/.terraformrc` and `~/.tofurc` — a shared provider cache and checkpoint
  off, plus the cache directory itself.

### Per-hat tool isolation

By default each hat gets its own configuration for the tools below, so commands
such as `kubectl config use-context`, `aws sso login`, `terraform init`, `az login` or `gh auth login` only
affect the shell that ran them.

| Tool         | Per-hat location                                 | Selected by                                      | Opt out                             |
| ------------ | ------------------------------------------------ | ------------------------------------------------ | ----------------------------------- |
| kubectl      | `~/.kube/config.<hat>`                           | `KUBECONFIG`                                     | `kube: { isolate: false }`          |
| AWS CLI      | `~/.aws/.hats/<hat>.config`, `<hat>.credentials` | `AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE` | `aws: { isolate: false }`           |
| Terraform    | `~/.terraform.d/.hats/<hat>.tfrc`                | `TF_CLI_CONFIG_FILE`                             | `terraform: { isolate: false }`     |
| Azure CLI    | `~/.azure/.hats/<hat>/`                          | `AZURE_CONFIG_DIR`                               | `azure: { isolate: false }`         |
| GitHub CLI   | `~/.config/gh/.hats/<hat>/`                      | `GH_CONFIG_DIR`                                  | `github: { isolate: false }`        |
| k9s          | `~/.config/k9s/hats/<hat>/`                      | `K9S_CONFIG_DIR`                                 | `k9s: { isolate: false }`           |
| coder        | `~/.config/coderv2/hats/<hat>/`                  | `CODER_CONFIG_DIR`                               | `coder: { isolate: false }`         |
| VS Code      | a profile named after the hat                    | `code --profile <hat>`, then VS Code remembers   | `features.vscode`                   |
| SSH          | `~/.ssh/config.d/<hat>.conf`                     | `HATS_HAT`, expanded in `~/.ssh/config`          | `features.ssh`                      |
| git          | `~/.gitconfig.d/<hat>`                           | `include.path` via `GIT_CONFIG_KEY_0`            | n/a                                 |

#### SSH

`~/.ssh/config` includes `~/.ssh/config.d/${HATS_HAT}.conf`, then
`~/.ssh/config.d/common.conf` for hosts shared by all hats, then the
`Host *` defaults. ssh expands the variable per process, so each shell sees only
the hosts and keys of its active hat; `github.com` can use a different key in
each terminal without wrapper scripts. These files are machine-local, and
`~/.ssh/config.d/README` describes the layout. Requires OpenSSH 9.9 or later,
which `hats doctor` checks.

#### git

Settings in `~/.gitconfig.d/<hat>`, such as `url.insteadOf` or
`core.sshCommand`, apply only in shells wearing that hat.

#### k9s

Each hat's k9s directory links to the shared `~/.config/k9s` files (which
bosun themes), so plugins and aliases are kept separate per hat while the
theme is shared.

#### coder

`CODER_CONFIG_DIR` also moves the session token out of the macOS
Keychain and into the hat's directory, so each hat has its own login.
`coder: { url: https://coder.acme.com }` exports `CODER_URL` for the hat. hats
sets `CODER_SSH_CONFIG_FILE`, so `coder config-ssh` writes workspace hosts into
the hat's `~/.ssh/config.d/<hat>.conf` rather than the shared `~/.ssh/config`.

#### Terraform

hats scaffolds `~/.terraformrc` and `~/.tofurc` with the same settings: a
provider cache shared by both tools at `~/.cache/opentofu/plugin-cache`
(created by `hats hat sync`, since neither tool creates it), and Terraform's
checkpoint calls switched off. Tokens stay out of both files; give a hat
`TF_TOKEN_<host>: { secret: <key> }` in its `env:` instead.

`TF_CLI_CONFIG_FILE` points each hat at its own `.terraform.d/.hats/<hat>.tfrc`,
seeded once from the shared `~/.terraformrc` (or `~/.tofurc`, on a machine with
only that). OpenTofu honours the variable too, so the one file serves both
tools. Seeding is one-shot: a later change to the shared files does not reach
a hat that already has its copy. Setting the variable also makes Terraform
skip the shared `~/.terraform.d` directory, so a hat's `credentials`,
`credentials_helper`, `plugin_cache_dir` and `provider_installation` settings
stand alone. Note that `terraform login` always writes its token to the shared
`~/.terraform.d/credentials.tfrc.json`, which the hat no longer reads; a hat
that wants HCP tokens per-hat writes a `credentials` block into its own tfrc
or exports `TF_TOKEN_app_terraform_io`.

#### Azure CLI

`AZURE_CONFIG_DIR` points each hat at its own `~/.azure/.hats/<hat>/` directory,
isolating the token cache and cloud configuration from `az login` and `az account set`.

#### GitHub CLI

`GH_CONFIG_DIR` points each hat at its own `~/.config/gh/.hats/<hat>/` directory,
isolating authentication tokens and host configuration from `gh auth login`.

### VS Code profiles

Editor settings are not environment variables, so VS Code is handled with its
own mechanism: a profile per hat, named after the hat. Each profile has its own
global `settings.json`, seeded from yours, and shares everything else with the
default profile, extensions and their state included. Turn this on with
`features.vscode`.

**hats creates and removes the profiles**, because a profile can only be
registered while VS Code is closed: VS Code keeps its profile list in memory
and rewrites it whenever anything changes.

- `hats hat create` and `hats hat sync` register the missing ones. With
  VS Code open, `create` offers to quit or force stop it, and otherwise leaves
  the profile for the next sync. `hats doctor` lists what is outstanding.
- `hats hat delete` unregisters the profile and moves its directory to
  `~/.hats/backups/`, once VS Code is stopped. A profile you made by hand with
  the hat's name is never touched.

**Three things then choose the profile for a folder:**

- VS Code remembers the profile a folder was last opened with, and reopens it
  that way, whether the profile was chosen by hand or by `code`.
- In a shell wearing a hat, `code <path>` opens with that hat's profile, which
  is also how a profile that is still missing gets registered: VS Code is
  closed at that moment.
- A folder that belongs to one hat can say so in `.vscode/settings.json`:

  ```json
  { "hats.hat": "acme" }
  ```

  The hats extension (in [vscode/hats](vscode/hats), packaged as a `.vsix` on
  each release; install it with `code --install-extension`) then offers to
  switch profile whenever that folder is opened under another one, and warns
  whenever a `.hat` file or active hat is detected so the profile can be
  created or managed through VS Code's own profile manager. It cannot switch
  by itself: VS Code's switch command always asks, and offers no API to a
  profile.

Worth knowing:

- A folder that is already open in a window is only focused, so its profile
  does not change.
- Opening a folder from another hat's shell changes the profile VS Code
  remembers for it.
- `EDITOR="code --wait"`, run by git, does not go through the shell function.
- A window takes the environment of the shell that ran `code`, merged over
  VS Code's own. After a restart or a Dock launch, windows keep their profile
  but get the login shell's environment, so run `hat <name>` in their
  terminals.
- VS Code Insiders, OSS builds and snap or flatpak installs keep their state
  elsewhere and are not covered.

### Per-hat files

`hats hat create` creates these files along with the hat, and `hats hat sync`
creates any that are missing for existing hats:

- ssh and git: a file containing a comment line
- AWS, kube and Terraform: a copy of the shared config file
- k9s: links to the shared config and theme
- coder, azure, github: an empty directory, mode 0700
- VS Code: a profile registered with VS Code, its `settings.json` a copy of
  your default one, mode 0700 (only with `features.vscode`, and only while
  VS Code is closed)

After creation, hats does not modify these files. `hats hat delete <name>` is
the only command that removes them, and it moves them to `~/.hats/backups/`
rather than deleting them. If you remove a hat from `config.yaml` by hand, hats
can no longer find its files, so use `hats hat delete` instead.

## Commands

|                                  |                                                             |
| -------------------------------- | ----------------------------------------------------------- |
| `hat [name]`                     | Switch this shell. Without a name, shows a picker.          |
| `hats init`                      | The wizard: identity, features, hats, secrets               |
| `hats hat list`                  | List hats, marking the active one                           |
| `hats hat show <name>`           | Show one hat with inheritance resolved                      |
| `hats hat current`               | Show the hat active in this shell                           |
| `hats hat sync`                  | Create every missing skeleton, per-hat file and profile     |
| `hats hat create <name>`         | Add a hat and create its files, via flags or prompts        |
| `hats hat delete <name>`         | Remove a hat and move its files to backups                  |
| `hats hat vscode-profile <name>` | Say whether VS Code has that hat's profile; used by `code`  |
| `hats env <name>`                | Print the shell code a switch would run, without running it |
| `hats env --here`                | The same for the nearest `.hat` file; used by the `cd` hook |
| `hats secrets fetch`             | Download tokens from the vault                              |
| `hats doctor`                    | Check this machine has what hats needs                      |
| `hats test`                      | Run end-to-end tests of the switcher in real shells         |
| `hats shell-init zsh`            | Print the `hat` function and its completion                 |
| `hats completions <shell>`       | Print the completion script for `hats`                      |
| `hats version`                   | Show the binary version                                     |

Every command accepts `--help`. The dotfiles commands that used to live here
(`plan`, `apply`, `brew`, `hooks`, `lint`, `update`) are
[bosun](https://github.com/planesailingio/bosun)'s now.

## Secrets

Tokens are fetched from Bitwarden or a self-hosted Vaultwarden into
`~/.hats/secrets.yaml` (mode 0600). Shells read from this file, so switching
hats does not require network access.

Vault items are selected by label rather than name: add a custom field named
`hats` whose value is the secret key, or place the item in a folder named
`hats`. Adding a secret requires no change to this repository.

The vault credentials can be encrypted to a key generated in a YubiKey's PIV
applet, which cannot be exported. `age-plugin-yubikey`, which performs the
decryption, is installed as a dependency of hats.

```sh
hats secrets enrol-yubikey     # generate the key and encrypt the credentials
hats secrets fetch             # touch the YubiKey when it flashes
```

Without a YubiKey, `hats secrets fetch` prompts for the credentials each time
and does not store them. On Linux, the plugin also requires the `pcscd` system
service to access the smartcard; `hats doctor` checks for it.

`secrets.expected` in the config lists keys nothing in a hat refers to but
`hats secrets status` should still warn about — `git_signing_key`, which only
the scaffolded `~/.gitconfig` reads, is seeded there by the wizard.

## File layout

| Path                      | Contents                                                                          |
| ------------------------- | --------------------------------------------------------------------------------- |
| `cli/`                    | The `hats` source code, in Rust.                                                  |
| `vscode/hats/`            | The VS Code extension.                                                            |
| `~/.hats/config.yaml`     | Your hats, identity, features and endpoints. Machine-local, not in git.           |
| `~/.hats/secrets.yaml`    | Fetched tokens, mode 0600.                                                        |
| `~/.hats/backups/`        | Files a deleted hat owned, and replaced configs.                                  |
| `~/.ssh/config.d/`        | SSH hosts and keys: `<hat>.conf` per hat, `common.conf` for all. Not in git.      |
| `~/.gitconfig.d/`         | Per-hat git settings in `<hat>`, included by shells wearing that hat. Not in git. |
| `~/.config/k9s/hats/`     | Per-hat k9s config in `<hat>/`, linked to the shared theme. Machine-local.        |
| `~/.config/coderv2/hats/` | Per-hat coder login in `<hat>/`: URL and session token, mode 0700.                |

## Migrating from hats ≤ 0.10

Version 0.11 split the dotfiles engine out into bosun. On an existing machine:

1. `brew upgrade hats && brew install planesailingio/tools/bosun`
2. Add a `features:` block to `~/.hats/config.yaml` if you want non-default
   values (the defaults match the old `ssh` and `editor` groups). Stale
   `groups:` and `meta.repo` keys are ignored.
3. `rm -rf ~/.hats/repo ~/.hats/state.yaml ~/.hats/plans`
4. `bosun init && bosun plan` — the plan should show almost no file changes,
   which is the migration check. The first apply re-runs hooks once; they are
   idempotent. Your existing `~/.gitconfig` keeps its inline styling; trim it
   when bosun's `~/.config/git/style.gitconfig` lands, or leave the duplicate,
   which git tolerates.

## Versioning

Plain semver on the binary. After `brew upgrade hats` there is nothing else to
run: the repo-tag lockstep went to bosun with the dotfiles.

## Development

```sh
cargo fmt --all             # CI runs --check, so format before pushing
cargo test                  # unit, snapshot and integration tests
cargo clippy --all-targets --all-features -- -D warnings
.devcontainer/test.sh       # Linux smoke test (run inside the dev container)
```

## Licence

MIT. See [LICENSE](LICENSE).
