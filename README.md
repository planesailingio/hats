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

**What hats does.** `profile acme` sets git identity, signing key, AWS
profile, kube context, tokens, toolchain versions and the terminal's background
tint — in *that shell only*. The window next door doesn't move.

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
| AWS CLI | `$AWS_PROFILE`, falling back to `~/.aws/config` | this shell, or every terminal |
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
(shell, git, ssh, theme, toolchains…), then walks you through a profile at a
time: git name and email, AWS profile and region, kube context, terminal tint.
Add as many as you have clients. The second and subsequent ones can inherit from
the first, so "same me, different cloud account" is a two-line profile.

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
profile
```

That's it — a fuzzy picker of your profiles, or name one directly. Set your
terminal font to a [Nerd Font](https://www.nerdfonts.com/) so the prompt symbols
render.

> Configured a secrets backend in step 2? `hats secrets fetch` pulls your tokens
> down. Anything not working? `hats doctor` will say what's missing.

## What it looks like

Put a hat on. One line back, telling you who you now are:

```console
$ profile acme
⛭ profile: acme  (git=jane.doe@acme.com  aws=acme-aws  kube=acme)
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

## A profile is about ten lines

`~/.hats/config.yaml`, machine-local, never in git:

```yaml
profiles:
  normal:
    colour: "#2a2040"
    identity: { name: Jane, email: jane@example.com }

  acme:
    inherits: normal
    colour: "#331420"
    identity: { email: jane.doe@acme.com }
    aws:  { profile: acme-aws, region: eu-west-2 }
    kube: { context: acme }
    env:
      JIRA_TOKEN: { secret: acme/jira }
```

`inherits` folds the parent in, so a child records only the *difference*. Each
profile gets its own copy of `~/.kube/config` by default — that's rule 2, below —
so a stray `use-context` can only ever affect the shell that ran it. And
`{ secret: ... }` is a reference, not a value: the token itself never goes near
this file.

## The four rules it's built on

1. Per-context state lives in **environment variables**, never in shared files.
2. For a tool that insists on a file, give each context **its own copy** and
   point an environment variable at it.
3. **Unset before you set**, or the last context leaks into the next.
4. **Put the active context in your prompt**, and colour production red.

Rule 3 is the one that bites, and it's the reason a hand-written reset list
doesn't work: the list has to be updated every time any profile gains a
variable, and eventually it isn't. A forgotten `AWS_PROFILE` or Jira token then
survives every switch, silently, until something notices.

hats derives the reset list from the union of every profile's variables, so a
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
| `profile [name]` | Switch this shell. No name gives you a picker. |
| `hats plan` | What would change in your home directory |
| `hats apply` | Do it |
| `hats secrets fetch` | Pull your tokens down from the vault |
| `hats doctor` | Check this machine has what hats needs |

<details>
<summary><b>The rest</b> — profiles, packages, maintenance, authoring</summary>

<br>

**Profiles**

| | |
|---|---|
| `hats profile list` | Every profile, active one marked |
| `hats profile show <name>` | One profile with its inheritance folded in |
| `hats profile current` | Which hat this shell is wearing |
| `hats env <name>` | The shell code a switch would run, printed not executed |

**Dotfiles**

| | |
|---|---|
| `hats diff` | The diff alone, without the hook plan |
| `hats render <file>` | Render one template and print it, or syntax-check it |
| `hats hooks` | List the repo's hooks, or run one by name |
| `hats lint` | Check the manifest, templates, profiles and shell scripts |

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
| `hats shell-init zsh` | The `profile` function and its completion |
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
`~/.hats/secrets.yaml` at mode 0600. Shells read that file, so switching profile
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
| `~/.hats/config.yaml` | **Your** profiles, identities and endpoints. Machine-local, never in git. |
| `~/.hats/secrets.yaml` | Fetched tokens, mode 0600. |

Profiles are deliberately not in this repo. It ships defaults anyone can use;
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
cargo test                  # unit, snapshot and integration tests
cargo clippy --all-targets --all-features -- -D warnings
hats lint                   # manifest, templates, profiles, shell scripts
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
