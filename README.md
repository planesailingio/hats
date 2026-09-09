<div align="center">

# 🎩 hats

**One laptop, many hats. Change who you are with one command — in this terminal only.**

[![CI](https://github.com/planesailingio/hats/actions/workflows/ci.yml/badge.svg)](https://github.com/planesailingio/hats/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/tag/planesailingio/hats?label=release&sort=semver)](https://github.com/planesailingio/hats/releases)
[![Licence](https://img.shields.io/github/license/planesailingio/hats)](LICENSE)
![macOS & Linux](https://img.shields.io/badge/macOS%20%C2%B7%20Linux-supported-informational)

</div>

---

## TL;DR

**The problem.** `kubectl config use-context` doesn't change your shell — it
edits a file that every terminal on the machine shares. So does
`git config --global`. Switch client and you're updating four different
mechanisms by hand, then checking all four before you type anything dangerous.

**What hats does.** `hat acme` sets git identity, signing key, AWS
credentials, kube context, tokens, toolchain versions and the terminal's
background tint — in *that shell only*. The window next door doesn't move.

**The other half.** The dotfiles behind it are managed like infrastructure:
`hats plan` shows a line-level diff of what would change in your home directory,
`hats apply` writes it and backs up whatever it replaced.

macOS and Linux, one binary, no runtime:

```sh
brew install planesailingio/tools/hats && hats init
```

## The problem

Every tool on your machine remembers who you are. No two of them agree on where
to keep the answer, or on how far it reaches.

| Tool | Where its answer lives | Reaches |
|---|---|---|
| git | `~/.gitconfig` | every terminal |
| kubectl | `~/.kube/config` | every terminal |
| AWS CLI | `~/.aws/config` and `~/.aws/credentials` | every terminal |
| npm, pip | `~/.npmrc`, a configured index URL | every terminal |
| node, python | whatever the version manager last made default | every terminal |
| API tokens | wherever you last exported them | this shell, until you close it |

Three things follow from that table, and all three are the same bug:

1. **Commands that look like mode switches are global edits.** `kubectl config
   use-context` and `git config --global user.email` do not change the shell you
   ran them in. They rewrite a file every other terminal is also reading —
   including the ones you can't currently see.
2. **One switch is really five,** done by hand, in the right order, every time.
   Miss one and the mismatch is silent.
3. **Nothing unsets.** Yesterday's token, the last client's registry and a stale
   `AWS_PROFILE` survive into the next context, because clearing them was never
   anybody's job.

Which leaves the actual daily cost — not the incident, the vigilance. Before
anything with consequences: `git config user.email`, `echo $AWS_PROFILE`,
`kubectl config current-context`. A checklist per switch, many switches a day,
and every checklist gets skipped eventually.

The workarounds each solve a quarter of it. `includeIf gitdir:` handles git and
only git, and only if your repos live where it expects. direnv keys off the
directory you're standing in, but which client you're being is a property of
*you right now*, not of a folder. `kubie` gets the kube half genuinely right and
doesn't claim to do the rest. Run all three and you have three mechanisms that
can disagree.

hats is the boring fix: one command, one line of confirmation back, and the
answer parked in your prompt from then on. The property the whole design rests
on is that **a context switch in one terminal cannot leak into another.**

## Why "hats"?

Because that's already the metaphor everyone uses. You wear a different hat for
each client — same person, different role — and the whole trick is only ever
wearing *one at a time*.

Here, each terminal wears its own. Swap hats in this window and the one next to
it keeps the hat it had. It's also four letters, which matters when you type it
forty times a day, and `hats plan` / `hats apply` read like sentences.

## Quick start

**1. Install it.**

```sh
brew install planesailingio/tools/hats
```

On a brand-new Mac with nothing on it — no Xcode tools, no Homebrew — start
here instead, and it'll fetch all three and run step 2 for you:

```sh
sh -c "$(curl -fsLS https://raw.githubusercontent.com/planesailingio/hats/main/bootstrap.sh)"
```

**2. Run the wizard.**

```sh
hats init
```

It clones the dotfiles into `~/.hats/repo`, asks which groups of files you want
(shell, git, ssh, theme, toolchains…), then walks you through a hat at a
time: git name and email, kube context, terminal tint.
Add as many as you have clients. The second and subsequent ones can inherit from
the first, so "same me, different cloud account" is a two-line hat.

Nothing has touched your home directory yet.

**3. Look before you leap.**

```sh
hats plan
```

Every file that would be created, changed or removed, with the diff. If you
don't like it, edit `~/.hats/config.yaml` and run it again.

**4. Apply it.**

```sh
hats apply
```

Files written, anything replaced is backed up first, setup hooks run.

**5. Open a fresh terminal and put a hat on.**

```sh
hat
```

That's it — a fuzzy picker of your hats, or name one directly. Set your
terminal font to a [Nerd Font](https://www.nerdfonts.com/) so the prompt symbols
render.

> Configured a secrets backend in step 2? `hats secrets fetch` pulls your tokens
> down. Anything not working? `hats doctor` will say what's missing.

### Using it day to day

Once it's set up, almost everything is these four:

```sh
hat                      # switch this terminal — picker, or `hat <name>`
hats hat current         # which hat am I wearing?
hats plan                # I changed a dotfile or my config — what would that do?
hats apply               # do it
```

The rhythm is: **open a terminal, put a hat on, work.** New window for a
different client means a new `hat` in that window. Nothing you do in one
terminal reaches another, so you can leave a production shell open next to a
personal one without them interfering.

**Changing a hat** — new token, different AWS account, another client
entirely — is an edit to the `hats:` block in `~/.hats/config.yaml`, then
`hat <name>` again in any shell that needs it. No apply required: a switch
reads the config fresh every time. (The top-level `identity:` block is the
exception. It's the fallback that renders into `~/.gitconfig`, so changing that
one does want a `hats apply`.)

**Changing a dotfile** is the other direction: edit it in `~/.hats/repo`, then
`hats plan` to see what would land and `hats apply` to write it. Open shells keep
the old version until they're restarted, as with any dotfile.

**Adding a token** is a change in your vault, not here: give the item a custom
field called `hats`, then `hats secrets fetch` and reference it from a hat as
`{ secret: <key> }`.

> `hat` is a shell function rather than a hats subcommand, because no program
> can change the environment of the shell that started it — it gets a copy that
> dies with it. `hats shell-init zsh` installs the function from your `.zshrc`,
> and `hats env <name>` prints what a switch would do without doing it. If
> `hat` isn't found, you're in a shell that started before `hats apply` ran;
> open a new one.

## What it looks like

Put a hat on. One line back, telling you who you now are:

```console
$ hat acme
⛭ hat: acme  (git=jane.doe@acme.com  kube=acme)
```

Then the answer stays in front of you permanently, and the kube context is
coloured by how much trouble it can cause:

```console
╭─jane@laptop ~/git/acme/platform ‹main ✔›  󱃖 acme  ☸ staging  acme-aws
╰─➤ terraform apply

╭─jane@laptop ~/git/acme/platform ‹main ✔›  󱃖 acme  🚨 ☸ prod-eu-west-1  acme-aws
╰─➤ ▏
```

Green for dev, yellow for staging, bright red and a siren for production. You
stop reading it consciously after a week and start noticing it peripherally,
which is exactly the point. It's a hardware interlock for your hands.

And the dotfile side, which should never surprise you:

```console
$ hats plan
  ~ ~/.zshrc                           update  (+4 −12)
      -bindkey -e
      +bindkey -e   # Emacs-style line editing
  + ~/.config/starship.toml            create  (140 lines)
  - ~/.env.d/dev.zsh                   destroy

Plan: 1 to add, 1 to change, 1 to destroy, 0 permission changes; 1 hook to run.
```

If that reads like `terraform plan`, good. That was the idea.

## A hat is about ten lines

`~/.hats/config.yaml`, machine-local, never in git:

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

`inherits` folds the parent in, so a child records only the *difference*. And
`{ secret: ... }` is a reference, not a value: the token itself never goes near
this file.

Notice there's no `aws:` block. Each hat gets its own `~/.kube/config` and its
own `~/.aws/config` + `~/.aws/credentials` by default — that's rule 2, below —
so a stray `use-context`, or an `aws sso login`, can only ever affect the shell
that ran it. Turn either off per hat with `aws: { isolate: false }` or
`kube: { isolate: false }`.

## The four rules it's built on

1. Per-context state lives in **environment variables**, never in shared files.
2. For a tool that insists on a file, give each context **its own copy** and
   point an environment variable at it.
3. **Unset before you set**, or the last context leaks into the next.
4. **Put the active context in your prompt**, and colour production red.

Rule 3 is the one that bites, and it's the reason a hand-written reset list
doesn't work: the list has to be updated every time any hat gains a
variable, and eventually it isn't. A forgotten `AWS_PROFILE` or Jira token then
survives every switch, silently, until something notices.

hats derives the reset list from the union of every hat's variables, so a
variable added anywhere joins it automatically. The class of bug is gone rather
than fixed — which is the standard the rest of this holds itself to.

Each rule is argued out properly in
[One Laptop, Four Clients](docs/one-laptop-four-clients.md), including the
approaches that don't work and why. [Part 2](docs/one-laptop-four-clients-part-2.md)
is the implementation.

## Commands

Five of them cover almost every day:

| | |
|---|---|
| `hat [name]` | Switch this shell. No name gives you a picker. |
| `hats plan` | What would change in your home directory |
| `hats apply` | Do it |
| `hats secrets fetch` | Pull your tokens down from the vault |
| `hats doctor` | Check this machine has what hats needs |

<details>
<summary><b>The rest</b> — hats, packages, maintenance, authoring</summary>

<br>

**Hats**

| | |
|---|---|
| `hats hat list` | Every hat, active one marked |
| `hats hat show <name>` | One hat with its inheritance folded in |
| `hats hat current` | Which hat this shell is wearing |
| `hats env <name>` | The shell code a switch would run, printed not executed |

**Dotfiles**

| | |
|---|---|
| `hats diff` | The diff alone, without the hook plan |
| `hats render <file>` | Render one template and print it, or syntax-check it |
| `hats hooks` | List the repo's hooks, or run one by name |
| `hats lint` | Check the manifest, templates, hats and shell scripts |

**Packages** — see [bundles](#package-bundles)

| | |
|---|---|
| `hats brew install [bundle]` | Install a bundle (default: `full`) |
| `hats brew check <bundle>` | What's missing; installs nothing |
| `hats brew cleanup <bundle>` | Offer to remove what isn't in it |

**Maintenance**

| | |
|---|---|
| `hats update` | Move the repo to the tag matching this binary |
| `hats version` | Binary tag, repo tag, and whether they agree |
| `hats test` | Prove the switcher works: real commits, real shells |
| `hats shell-init zsh` | The `hat` function and its completion |
| `hats completions <shell>` | Completion script for hats itself |

</details>

Everything takes `--help`.

## Package bundles

Packages live in `brew/` as four files rather than one sprawling Brewfile. Every
bundle includes `core`, so you never end up on a machine without the base set.

| Bundle | What you get | For |
|---|---|---|
| `core` | shell, git, JSON/YAML/HTTP, system inspection, secrets, comms | any machine, whatever the job |
| `devops` | core + clusters, cloud CLIs, IaC, containers, infra scanners | infrastructure you operate |
| `pentest` | core + nmap, rustscan, sqlmap, Burp, sslscan | a target you're testing |
| `dev` | core + languages, service clients, code SAST, release tooling | code you write |
| `full` | everything above | the default |

```sh
hats brew install devops    # core + devops
hats brew check pentest     # what's missing; installs nothing
```

<details>
<summary>Why hats wraps <code>brew bundle</code> rather than calling it directly</summary>

<br>

`brew bundle` takes a single `--file`, so hats concatenates the bundle into one
temporary Brewfile before shelling out. That matters most for `cleanup`: pointed
at one file, brew would cheerfully offer to uninstall everything in the others.

The `brew-bundle` hook installs `${HATS_BREW_BUNDLE:-full}` on apply. Adding a
new bundle means touching three places: `brew/`, `BrewBundle` in
`cli/src/cli.rs`, and the hook's `onchange` inputs in `hats.yaml`.

</details>

## Secrets

Tokens come from Bitwarden — or a self-hosted Vaultwarden — into a single
`~/.hats/secrets.yaml` at mode 0600. Shells read that file, so switching hat
stays instant and works on a train.

Items are found **by label, not by name**: give a vault item a custom field
called `hats` whose value is the secret key, or drop it in a folder called
`hats`. Adding a secret is a change in your vault, not a commit here.

The credentials that reach the vault are themselves encrypted, to a key
generated inside a YubiKey's PIV applet that cannot leave it. The
`age-plugin-yubikey` that does the unsealing ships as a dependency of hats, so
there's nothing extra to go and find:

```sh
hats secrets enrol-yubikey     # generate the key, seal the credentials
hats secrets fetch             # touch the key when it blinks
```

Without a YubiKey, `hats secrets fetch` asks for the credentials each time and
stores nothing. On Linux the plugin also needs a running `pcscd` to reach the
smartcard — a system service rather than a Homebrew package. `hats doctor` will
tell you.

## Where things live

| Path | What |
|---|---|
| `hats.yaml` | Which files exist, how they group, which hooks run. Generic: no names, no emails, no secrets. |
| `files/` | The dotfiles themselves, mirroring `$HOME`. A `.j2` suffix means it's a template. |
| `brew/` | The four package bundles. |
| `hooks/` | Setup scripts (Homebrew, the bundles, zsh, macOS defaults, the Dock). |
| `cli/` | The `hats` source, in Rust. |
| `~/.hats/config.yaml` | **Your** hats, identities and endpoints. Machine-local, never in git. |
| `~/.hats/secrets.yaml` | Fetched tokens, mode 0600. |

Hats are deliberately not in this repo. It ships defaults anyone can use;
who you work for stays on your laptop.

## Version lockstep

The binary and the dotfiles ship from one git tag. `~/.hats/repo` is checked out
at exactly the tag matching the running `hats`, so a given version can only ever
drive the dotfiles it was released with. No "works on my machine, and my machine
is three months of dotfile commits ahead of yours".

```sh
brew upgrade hats     # binary moves
hats update           # repo follows
```

`hats update --check` reports the pair: exit 0 in step, 3 repo behind, 4 binary
behind.

## Developing

```sh
cargo fmt --all             # CI fails on --check, so run this before pushing
cargo test                  # unit, snapshot and integration tests
cargo clippy --all-targets --all-features -- -D warnings
hats lint                   # manifest, templates, hats, shell scripts
hats test --container       # the Linux suite, in Docker
```

Working against a checkout rather than the managed clone:

```sh
HATS_DEV=1 hats --hats-home /tmp/hats plan
```

Releasing is documented in [docs/releasing.md](docs/releasing.md); every change
is in [CHANGELOG.md](CHANGELOG.md).

## Licence

MIT. See [LICENSE](LICENSE).
