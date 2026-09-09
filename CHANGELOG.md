# Changelog

All notable changes to `hats` and these dotfiles. The binary and the dotfiles
content ship from one tag, so one entry covers both.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

`hats`, a Rust CLI that replaces chezmoi, the Makefile, six `run_` scripts, a
Bitwarden fetch script and four hand-written zsh profile files.

### Added

- **Plan and apply.** `hats plan` shows what would change before anything is
  written: per-file create, update, destroy or permission change, with
  line-level unified diffs and a terraform-style summary. It exits 2 when there
  is work to do. `hats apply` re-plans, asks, then writes atomically, backing up
  everything it replaces into `~/.hats/backups/<timestamp>/`.
- **Destroy detection.** A file managed on the last apply but not on this one is
  reported and, if it is unmodified since hats wrote it, moved to the backup
  directory. A file edited by hand is flagged and kept unless `--force-prune`.
- **Profiles as data.** Identity, AWS, kube context, environment and terminal
  tint live in `~/.hats/config.yaml`. `hats env` emits the shell for one switch
  and `hats shell-init zsh` installs the `profile` function that evaluates it,
  with an fzf picker and completion.
- **Secrets.** `hats secrets fetch` pulls from Bitwarden or a self-hosted
  Vaultwarden into a single `~/.hats/secrets.yaml` at mode 0600. Items are found
  by label: a custom field named `hats` whose value is the secret key, or
  membership of a folder called `hats`. No vault item names live in the repo.
- **YubiKey-protected credentials.** `hats secrets enrol-yubikey` generates a key
  in a PIV slot and seals the vault credentials to it with age. Decrypting needs
  a physical touch. Without a YubiKey, credentials are prompted for each fetch
  and never stored.
- **Version lockstep.** The dotfiles clone sits detached at the tag matching the
  running binary. `hats update` moves it there and takes no version argument;
  `hats update --check` exits 0 in step, 3 repo behind, 4 binary behind.
- **`hats init`** clones the repo, asks one yes/no per group of files, loops
  collecting profiles until told to stop, and asks how secrets should be
  fetched. Every question runs through one prompter, so `--non-interactive` and
  `--answers <file>` drive the identical code path.
- **`hats test`** replaces the container shell suite: it evaluates each profile
  in a real shell, makes a real commit and checks the author, and proves two
  shells get separate kubeconfigs while the shared one does not move.
- **`hats doctor`, `hats lint`, `hats render`, `hats hooks`, `hats brew`** cover
  the remaining Makefile targets, with one help page and no `make -C` needed.
- **VS Code extensions** are managed. The list existed before but nothing read
  it; it is now a managed file with a hook behind it.

### Changed

- Templates moved from Go templates to minijinja, and the tree from chezmoi's
  `dot_`/`private_`/`.tmpl` naming to `files/` with literal names and a `.j2`
  suffix. Modes are declared in `hats.yaml` rather than encoded in filenames.
- The five-line Homebrew-prefix conditional that was copy-pasted into every
  template is now a single `{{ brew_prefix }}`, resolved once.
- `run_once_`/`run_onchange_` scripts became `hooks/` with triggers declared in
  the manifest. An `onchange` hook names its inputs, so editing the hook script
  itself re-runs it, which the filename convention could not express.
- Secrets are no longer baked into five rendered dotfiles. One 0600 file holds
  them, keeping shells offline and instant while shrinking the plaintext to a
  single path.
- Starship reads `HATS_PROFILE`, falling back to `DEV_PROFILE`.

### Fixed

- **The profile variable leak.** `_profile_reset` maintained its unset list by
  hand and missed `AWS_PROFILE`, `AWS_REGION`, `AWS_DEFAULT_REGION`, `JIRA_*`
  and `GITHUB_TOKEN`, so switching profiles left the previous client's values in
  the shell. The list is now derived from the union of every profile's keys, so
  a variable added to any profile joins it with no other edit. A test asserts
  that property directly.
- **The Jira token exported as `GITHUB_TOKEN`.** The acme profile set
  `GITHUB_TOKEN` from `jira_acme_api_token`, which broke `gh` and leaked a
  Jira credential to every tool that reads `GITHUB_TOKEN`. Profiles are data
  now, and the mapping is simply absent.
- **The fetch-secrets path bug.** `bin/fetch-secrets.sh` combined
  `chezmoi source-path` (already `.chezmoiroot`-resolved) with `/home`,
  producing `…/chezmoi/home/home/…` whenever chezmoi was on PATH. That script is
  gone.
- Rendered files keep their trailing newline. Jinja drops it by default, which
  would have silently corrupted shell rc files and ssh configs.
- Env bundles (`dev`, `staging`, `prod`) no longer call a zsh function that no
  longer exists; they set their own isolated `KUBECONFIG` directly.

### Removed

- chezmoi, and its `.chezmoiroot`, `.chezmoi.toml.tmpl`, `.chezmoidata/` and
  `.chezmoiignore` scaffolding.
- The `Makefile`, `bin/fetch-secrets.sh` and `bin/check-empty-secrets.sh`. There
  is no secrets file in the repo any more, so there is nothing for the
  pre-commit guard to guard.
- `~/.profiles.d/`. Profiles are data, and the shell integration is generated.

### Notes

- This repository starts from a fresh history. The previous dotfiles repo had a
  GitHub personal access token committed in `2a46f4c`; nothing from that history
  is carried over here. Revoke that token on GitHub if it is still live.
- The tap job authenticates as the `all-ci-workflows` GitHub App, using this
  repository's `TAP_APP_ID` variable and `TAP_APP_PRIVATE_KEY` secret.
