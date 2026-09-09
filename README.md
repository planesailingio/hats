# dotfiles

My dotfiles, and `hats` — the CLI that manages them.

One laptop, several clients. Each one wants a different git identity, a
different AWS account, a different Kubernetes cluster and its own tokens.
`hats` keeps those straight per terminal, so a context switch in one window
cannot leak into another.

```sh
brew install planesailingio/tools/hats
hats init
```

Or on a completely fresh Mac:

```sh
sh -c "$(curl -fsLS https://raw.githubusercontent.com/planesailingio/hats/main/bootstrap.sh)"
```

## What it does

**Manages your dotfiles like infrastructure.** `hats plan` shows exactly which
files would appear, change or be removed, with a line-level diff, before
anything is written. `hats apply` carries it out, backing up whatever it
replaces.

```
  ~ ~/.zshrc                           update  (+4 −12)
      -bindkey -e
      +bindkey -e   # Emacs-style line editing
  + ~/.config/starship.toml            create  (140 lines)
  - ~/.env.d/dev.zsh                   destroy

Plan: 1 to add, 1 to change, 1 to destroy, 0 permission changes; 1 hook to run.
```

**Switches client context per shell.** `profile acme` in one terminal
changes that terminal only: git identity, AWS profile, Kubernetes context,
tokens, and the background tint. The terminal next door keeps whatever it had.

```sh
profile              # fuzzy-pick from your profiles
profile acme    # or name one
hats profile list    # see them all, with the active one marked
```

## The four rules it is built on

1. Per-context state lives in **environment variables**, never in shared files.
2. For a tool that insists on a file, give each context **its own copy** and
   point an environment variable at it.
3. **Unset before you set**, or the last context leaks into the next.
4. **Put the active context in your prompt**, and colour production red.

Rule 3 used to be a hand-written list of variables to clear, and it drifted:
`AWS_PROFILE` and a client's Jira token were missing from it, so they survived a
profile switch. `hats` derives that list from the union of every profile's
variables, so a variable added to any profile joins it automatically. The bug
cannot come back.

The reasoning behind all four is in [docs/one-laptop-four-clients.md](docs/one-laptop-four-clients.md);
the mechanics are in [part 2](docs/one-laptop-four-clients-part-2.md).

## Where things live

| Path | What |
|---|---|
| `hats.yaml` | Which files exist, how they group, which hooks run. Generic: no names, no emails, no secrets. |
| `files/` | The dotfiles themselves, mirroring `$HOME`. A `.j2` suffix means it is a template. |
| `hooks/` | Setup scripts (Homebrew, the Brewfile, zsh, macOS defaults, the Dock). |
| `cli/` | The `hats` source. |
| `~/.hats/config.yaml` | **Your** profiles, identities and endpoints. Machine-local, never in git. |
| `~/.hats/secrets.yaml` | Fetched tokens, mode 0600. |

Profiles are deliberately not in this repo. It ships defaults everyone can use;
who you work for stays on your laptop.

## Commands

| | |
|---|---|
| `hats init` | Clone the repo, pick file groups, define profiles, choose a secrets backend |
| `hats plan` | Preview changes; exits 2 when there are any |
| `hats apply` | Write the files and run due hooks |
| `hats diff` | The diff alone, no hook plan |
| `hats profile` | List, show, or report the active profile |
| `hats secrets fetch` | Pull tokens from the vault into `~/.hats/secrets.yaml` |
| `hats update` | Move the repo to the tag matching this binary |
| `hats doctor` | Check this machine has what hats needs |
| `hats test` | Prove the switcher works: real commits, real shells |
| `hats lint` | Check the manifest, templates, profiles and scripts |

`hats --help` has the rest.

## Secrets

Tokens come from Bitwarden, or a self-hosted Vaultwarden, into a single
`~/.hats/secrets.yaml` at mode 0600. Shells read that file, so switching a
profile stays instant and works on a train.

Items are found **by label, not by name**: give a vault item a custom field
called `hats` whose value is the secret key, or drop it in a folder called
`hats`. Adding a secret is a change in your vault, not a commit here.

The credentials that reach the vault are themselves encrypted, to a key
generated inside a YubiKey's PIV applet:

```sh
hats secrets enrol-yubikey     # generate the key, seal the credentials
hats secrets fetch             # touch the key when it blinks
```

Without a YubiKey, `hats secrets fetch` asks for the credentials each time and
stores nothing.

## Version lockstep

The binary and the dotfiles ship from one git tag. `~/.hats/repo` is checked out
at exactly the tag matching the running `hats`, so a given version can only ever
drive the dotfiles it was released with.

```sh
brew upgrade hats     # binary moves
hats update           # repo follows
```

`hats update --check` reports the pair: exit 0 in step, 3 repo behind, 4 binary
behind.

## Developing

```sh
cargo test                  # unit, snapshot and integration tests
cargo clippy --all-targets --all-features -- -D warnings
hats lint                   # manifest, templates, profiles, shell scripts
hats test --container       # the Linux suite, in Docker
```

Working against a checkout rather than the managed clone:

```sh
HATS_DEV=1 hats --hats-home /tmp/hats plan
```

Releasing is documented in [docs/releasing.md](docs/releasing.md).
