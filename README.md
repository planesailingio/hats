<div align="center">

# 🎩 hats

**Per-terminal identity switching and managed dotfiles for macOS and Linux.**

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
gone on the actual work. Over the years I tried solving it with bash aliases and built up a collection of bash functions to help, but there had to be a better way while solving a couple of other itches along the way. `hats` is that better way.

With hats you can:

- Switch your shell identity  
  - git identity, SSH configs, cloud credentials, kube context, tokens
  and environment variables together with one command, `hat <name>`, in the
  current terminal only.
- Keep contexts isolated. Each hat gets its own copy of shared tool config, and
  the previous hat's variables are unset on every switch, so commands such as
  `kubectl config use-context` or `aws sso login` in one terminal never affect
  another.
- Manage your dotfiles like infrastructure. `hats plan` shows a line-level diff
  of what would change in your home directory, and `hats apply` writes it,
  backing up any file it replaces.
- Get an opinionated shell setup built on zsh and the starship prompt, with the
  CLI tools I use every day (see [Package bundles](#package-bundles)).

It ships as a single binary with no runtime dependencies.

```sh
curl -fsSL https://planesailingio.github.io/hats/install.sh | sh
```


## hats and alternatives

I only started using [chezmoi](https://www.chezmoi.io/) recently, and I really liked it, hats was inspired by it and grew
from it. Much of the dotfiles side of hats is modelled on it: a source
repository rendered into your home directory as real files, templates with OS
and architecture conditionals, and scripts that run once or when something they
depend on changes.

What chezmoi didn't give me was per-shell switching. It renders one state per
machine. Templates can vary by host, OS or user, but once applied every terminal
reads the same `~/.gitconfig`, `~/.kube/config` and `~/.aws/config`. That is a
deliberate scope, not a flaw: chezmoi manages files, and the problem above needs
environment variables set in the running shell. hats adds that layer on top of a
chezmoi-style dotfiles engine.

> Think of hats as the chezmoi for enviornment variables

|                             | chezmoi                                           | hats                                              |
| --------------------------- | ------------------------------------------------- | ------------------------------------------------- |
| Per-terminal identities     | No, one applied state per machine                 | Yes, `hat <name>`                                 |
| Preview before writing      | `chezmoi diff`, including scripts to run          | `hats plan`, Terraform-style with a summary line  |
| Templates                   | Go `text/template`                                | minijinja (Jinja2 syntax)                         |
| Scripts                     | `run_once_` / `run_onchange_` filename prefixes   | Hooks in `hats.yaml`; `onchange` names its inputs |
| Secrets                     | 1Password, Bitwarden, pass, Vault and many more   | Bitwarden or Vaultwarden, cached locally          |
| Platforms                   | macOS, Linux, Windows, BSDs                       | macOS and Linux; `hat` needs zsh                  |
| Dotfiles                    | Bring your own                                    | Ships an opinionated zsh and tool setup           |
| Maturity                    | Established, widely used, extensive documentation | New and pre-1.0; may still change                 |

If you want a mature, cross-platform dotfiles manager and work in one context at
a time, chezmoi is the better choice. If you need different identities active in
different terminals at the same time, that is the gap hats fills.

## Supported Tools

Most CLI tools store their active context in a file under your home directory, so
changing it in one terminal changes it in all of them:

| Tool         | Where the active context is stored normally    | Hats equivalent location                            | Selected by                                                   |
| ------------ | ---------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------------- |
| git          | `~/.gitconfig`                                 | `~/.gitconfig.d/<hat>`                              | `GIT_CONFIG_KEY_0`, plus `GIT_AUTHOR_*` and `GIT_COMMITTER_*` |
| kubectl      | `~/.kube/config`                               | `~/.kube/config.<hat>`                              | `KUBECONFIG`                                                  |
| AWS CLI      | `~/.aws/config` and `~/.aws/credentials`       | `~/.aws/.hats/<hat>.config` and `<hat>.credentials` | `AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE`              |
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
your prompt. Switching in one terminal does not affect any other.
## Quick start

### 1. Install

```sh
curl -fsSL https://planesailingio.github.io/hats/install.sh | sh
```

The script installs Homebrew if it is missing (after asking), then installs
hats. It does not write to your home directory. To use Homebrew directly:

```sh
brew install planesailingio/tools/hats
```

On a new Mac without Xcode command line tools or Homebrew, use the bootstrap
script instead. It installs all three and then runs `hats init`:

```sh
sh -c "$(curl -fsLS https://raw.githubusercontent.com/planesailingio/hats/main/bootstrap.sh)"
```

In containers and CI pipelines, set `CI=1` to skip the Homebrew prompt:

```sh
curl -fsSL https://planesailingio.github.io/hats/install.sh | CI=1 sh
```

### 2. Initialise

```sh
hats init
```

This clones the dotfiles into `~/.hats/repo`, asks which file groups to manage
(for example shell, git, ssh, theme and toolchains), and prompts for one or more hats: git
name and email, kube context and terminal tint. Additional hats can inherit from
an existing one. Nothing is written to your home directory at this stage.

### 3. Review the plan

```sh
hats plan
```

Lists every file that would be created, changed or removed, with a diff. Edit
`~/.hats/config.yaml` and re-run until the plan is what you want.

### 4. Apply

```sh
hats apply
```

Writes the files, backs up anything it replaces, and runs setup hooks.

### 5. Switch hat

Open a new terminal, then:

```sh
hat          # interactive picker
hat acme     # or switch directly
```

If you configured a secrets backend (experimental) during `hats init`, run `hats secrets fetch`
to download your tokens. `hats doctor` reports anything missing from the
machine.

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
✓ created ~/.config/k9s/hats/acme/skins  (→ ~/.config/k9s/skins)
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
hats update              # Updates the opinionated set of customisations and tooling
hats plan                # preview changes after editing a dotfile or config
hats apply               # apply them
```

Each terminal has its own hat. Opening a new terminal for a different context
means running `hat` in that terminal.

### Changing a hat

To change a hat's settings, such as a token or AWS account, edit its entry
under `hats:` in `~/.hats/config.yaml`, then run `hat <name>` again in any shell
that needs the change. `hats apply` is not needed, because each switch reads the
config when it runs. The exception is the top-level `identity:` block, which is
rendered into `~/.gitconfig` as the fallback identity and so requires
`hats apply`.

### Changing a dotfile

Edit the file in `~/.hats/repo`, run `hats plan` to review the change and
`hats apply` to write it. Shells that are already open keep the old version
until restarted or refreshed.

### Adding a token

Add the token to your vault (see [Secrets](#secrets)), run `hats secrets fetch`,
and reference it from a hat as `{ secret: <key> }`.

### The `hat` function

`hat` is a shell function, not a `hats` subcommand, because a child process
cannot modify its parent shell's environment. `hats shell-init zsh` defines the
function and is loaded from your `.zshrc`. `hats env <name>` prints the shell
code a switch would run without executing it. If `hat` is not found, the shell
was started before `hats apply` ran; open a new one.

## Prompt and plan output

Switching prints a one-line summary:

```console
$ hat acme
⛭ hat: acme  (git=jane.doe@acme.com  kube=acme)
```

The prompt then shows the active hat and kube context, coloured by
environment:

```console
╭─jane@laptop ~/git/acme/platform ‹main ✔›  󱃖 acme  ☸ staging  acme-aws
╰─➤ terraform apply

╭─jane@laptop ~/git/acme/platform ‹main ✔›  󱃖 acme  🚨 ☸ prod-eu-west-1  acme-aws
╰─➤ ▏
```

Development contexts are green, staging yellow, and production bright red with
a 🚨 marker.

`hats plan` output follows the same conventions as `terraform plan`:

```console
$ hats plan
  ~ ~/.zshrc                           update  (+4 −12)
      -bindkey -e
      +bindkey -e   # Emacs-style line editing
  + ~/.config/starship.toml            create  (140 lines)
  - ~/.config/k9s/skin.yml             destroy

Plan: 1 to add, 1 to change, 1 to destroy, 0 permission changes, 0 to scaffold; 1 hook to run.
```

## Configuring hats

Hats are defined in `~/.hats/config.yaml`, which is machine-local and not kept
in git:

```yaml
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

### Per-hat tool isolation

By default each hat gets its own configuration for the tools below, so commands
such as `kubectl config use-context`, `aws sso login` or `coder login` only
affect the shell that ran them.

| Tool    | Per-hat location                                 | Selected by                                      | Opt out                     |
| ------- | ------------------------------------------------ | ------------------------------------------------ | --------------------------- |
| kubectl | `~/.kube/config.<hat>`                           | `KUBECONFIG`                                     | `kube: { isolate: false }`  |
| AWS CLI | `~/.aws/.hats/<hat>.config`, `<hat>.credentials` | `AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE` | `aws: { isolate: false }`   |
| k9s     | `~/.config/k9s/hats/<hat>/`                      | `K9S_CONFIG_DIR`                                 | `k9s: { isolate: false }`   |
| coder   | `~/.config/coderv2/hats/<hat>/`                  | `CODER_CONFIG_DIR`                               | `coder: { isolate: false }` |
| SSH     | `~/.ssh/config.d/<hat>.conf`                     | `HATS_HAT`, expanded in `~/.ssh/config`          | n/a                         |
| git     | `~/.gitconfig.d/<hat>`                           | `include.path` via `GIT_CONFIG_KEY_0`            | n/a                         |

#### SSH

`~/.ssh/config` includes `~/.ssh/config.d/${HATS_HAT}.conf`, then
`~/.ssh/config.d/common.conf` for hosts shared by all hats, then the hats
`Host *` defaults. ssh expands the variable per process, so each shell sees only
the hosts and keys of its active hat; `github.com` can use a different key in
each terminal without wrapper scripts. These files are machine-local, and
`~/.ssh/config.d/README` describes the layout. Requires OpenSSH 9.9 or later,
which `hats doctor` checks.

#### git

Settings in `~/.gitconfig.d/<hat>`, such as `url.insteadOf` or
`core.sshCommand`, apply only in shells wearing that hat.

#### k9s

Each hat's k9s directory links to the managed theme, so plugins and
aliases are kept separate per hat while the theme is shared.

#### coder

`CODER_CONFIG_DIR` also moves the session token out of the macOS
Keychain and into the hat's directory, so each hat has its own login.
`coder: { url: https://coder.acme.com }` exports `CODER_URL` for the hat. hats
sets `CODER_SSH_CONFIG_FILE`, so `coder config-ssh` writes workspace hosts into
the hat's `~/.ssh/config.d/<hat>.conf` rather than the hats-managed
`~/.ssh/config`.

### Per-hat files

`hats hat create` creates these files along with the hat, and `hats apply`
creates any that are missing for existing hats:

- ssh and git: a file containing a comment line
- AWS and kube: a copy of the shared config file
- k9s: links to the managed theme
- coder: an empty directory, mode 0700

After creation, hats does not modify these files. `hats hat delete <name>` is
the only command that removes them, and it moves them to `~/.hats/backups/`
rather than deleting them. If you remove a hat from `config.yaml` by hand, hats
can no longer find its files, so use `hats hat delete` instead.

## Commands

Common commands:

|                      |                                                    |
| -------------------- | -------------------------------------------------- |
| `hat [name]`         | Switch this shell. Without a name, shows a picker. |
| `hats plan`          | Show what would change in your home directory      |
| `hats apply`         | Apply the plan                                     |
| `hats secrets fetch` | Download tokens from the vault                     |
| `hats doctor`        | Check this machine has what hats needs             |

<details>
<summary><b>All commands</b>: hats, dotfiles, packages, maintenance</summary>

<br>

**Hats**

|                          |                                                             |
| ------------------------ | ----------------------------------------------------------- |
| `hats hat list`          | List hats, marking the active one                           |
| `hats hat show <name>`   | Show one hat with inheritance resolved                      |
| `hats hat current`       | Show the hat active in this shell                           |
| `hats hat create <name>` | Add a hat and create its files, via flags or prompts        |
| `hats hat delete <name>` | Remove a hat and move its files to backups                  |
| `hats env <name>`        | Print the shell code a switch would run, without running it |

**Dotfiles**

|                      |                                                       |
| -------------------- | ----------------------------------------------------- |
| `hats diff`          | Show the diff without the hook plan                   |
| `hats render <file>` | Render one template and print it, or syntax-check it  |
| `hats hooks`         | List the repo's hooks, or run one by name             |
| `hats lint`          | Check the manifest, templates, hats and shell scripts |

**Packages** (see [Package bundles](#package-bundles))

|                              |                                            |
| ---------------------------- | ------------------------------------------ |
| `hats brew install [bundle]` | Install a bundle (default: `full`)         |
| `hats brew check <bundle>`   | Report what is missing without installing  |
| `hats brew cleanup <bundle>` | Offer to remove packages not in the bundle |

**Maintenance**

|                            |                                                       |
| -------------------------- | ----------------------------------------------------- |
| `hats update`              | Check out the repo tag matching this binary           |
| `hats version`             | Show binary and repo tags and whether they match      |
| `hats test`                | Run end-to-end tests of the switcher in real shells   |
| `hats shell-init zsh`      | Print the `hat` function and its completion           |
| `hats completions <shell>` | Print the completion script for `hats`                |

</details>

Every command accepts `--help`.

## Package bundles

Packages are defined in four files under `brew/`. Every bundle includes `core`.

| Bundle    | Contents                                                      | Intended for              |
| --------- | ------------------------------------------------------------- | ------------------------- |
| `core`    | shell, git, JSON/YAML/HTTP, system inspection, secrets, comms | any machine               |
| `devops`  | core + clusters, cloud CLIs, IaC, containers, infra scanners  | infrastructure work       |
| `pentest` | core + nmap, rustscan, sqlmap, Burp, sslscan                  | security testing          |
| `dev`     | core + languages, service clients, code SAST, release tooling | software development      |
| `full`    | all of the above                                              | default                   |

```sh
hats brew install devops    # core + devops
hats brew check pentest     # report what is missing; installs nothing
```

<details>
<summary>Why hats wraps <code>brew bundle</code></summary>

<br>

`brew bundle` accepts a single `--file`, so hats concatenates the bundle's files
into one temporary Brewfile before calling it. This matters for `cleanup`: run
against a single file, brew would offer to uninstall packages from the other
files.

The `brew-bundle` hook installs `${HATS_BREW_BUNDLE:-full}` on apply. Adding a
bundle requires changes in three places: `brew/`, `BrewBundle` in
`cli/src/cli.rs`, and the hook's `onchange` inputs in `hats.yaml`.

</details>

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

## File layout

| Path                      | Contents                                                                                      |
| ------------------------- | --------------------------------------------------------------------------------------------- |
| `hats.yaml`               | Managed files, their groups, and hooks. Contains no names, emails or secrets.                 |
| `files/`                  | The dotfiles, mirroring `$HOME`. Files ending in `.j2` are templates.                         |
| `brew/`                   | The four package bundles.                                                                     |
| `hooks/`                  | Setup scripts (Homebrew, bundles, zsh, macOS defaults, Dock).                                 |
| `cli/`                    | The `hats` source code, in Rust.                                                              |
| `~/.hats/config.yaml`     | Your hats, identities and endpoints. Machine-local, not in git.                               |
| `~/.hats/secrets.yaml`    | Fetched tokens, mode 0600.                                                                    |
| `~/.ssh/config.d/`        | SSH hosts and keys: `<hat>.conf` per hat, `common.conf` for all. Not in git.                  |
| `~/.gitconfig.d/`         | Per-hat git settings in `<hat>`, included by shells wearing that hat. Not in git.             |
| `~/.config/k9s/hats/`     | Per-hat k9s config in `<hat>/`, linked to the managed theme. Machine-local.                   |
| `~/.config/coderv2/hats/` | Per-hat coder login in `<hat>/`: URL and session token, mode 0700.                            |

The repository contains only generic defaults. Hat definitions, identities and
client details stay in machine-local files.

## Versioning

The binary and the dotfiles are released from the same git tag. `~/.hats/repo`
is checked out at the tag matching the installed `hats` binary, so each binary
version always runs against the dotfiles it was released with.

```sh
brew upgrade hats     # upgrade the binary
hats update           # move the repo to the matching tag
```

`hats update --check` reports whether they match: exit code 0 if in step, 3 if
the repo is behind, 4 if the binary is behind.

## Development

```sh
cargo fmt --all             # CI runs --check, so format before pushing
cargo test                  # unit, snapshot and integration tests
cargo clippy --all-targets --all-features -- -D warnings
hats lint                   # manifest, templates, hats, shell scripts
hats test --container       # Linux test suite, in Docker
```

To run against a local checkout instead of the managed clone:

```sh
HATS_DEV=1 hats --hats-home /tmp/hats plan
```

The release process is documented in [docs/releasing.md](docs/releasing.md).
Changes are listed in [CHANGELOG.md](CHANGELOG.md).

## Licence

MIT. See [LICENSE](LICENSE).
