# Changelog

All notable changes to `hats` and these dotfiles. The binary and the dotfiles
content ship from one tag, so one entry covers both.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.3.0 — 2026-09-09

One word for the thing you put on, and AWS isolated the way kube already was.

### Changed

- **AWS is isolated per hat instead of selected with `AWS_PROFILE`.** Each hat
  gets its own `~/.aws/.hats/<hat>.config` and `~/.aws/.hats/<hat>.credentials`,
  seeded once from the shared files and pointed at by `AWS_CONFIG_FILE` and
  `AWS_SHARED_CREDENTIALS_FILE`. This is rule 2 applied to AWS, and it fixes
  what `AWS_PROFILE` never could: `aws configure`, `aws configure set` and
  `aws sso login` all write to the shared files, so re-authenticating in one
  terminal rewrote state every other terminal was reading. Selecting a section
  inside a file being rewritten underneath you is not isolation.
- **Breaking: the `aws:` block is now isolation-only.** `profile:` and `region:`
  are gone; the block holds `isolate: true|false`, defaulting to true, exactly
  like `kube:`. Delete those two keys from every hat in `~/.hats/config.yaml`.
  Region belongs in the hat's own AWS config file now, where the CLI writes it.
- `hats env` gains `--no-aws`, matching `--no-kube`.
- `AWS_PROFILE`, `AWS_REGION` and `AWS_DEFAULT_REGION` are cleared on every
  switch even though hats no longer sets them: a hand-exported one would
  otherwise select a section inside the next hat's isolated file.
- The `hat` summary line drops `aws=`, and `hats hat show` reports
  `aws  isolated: <bool>` in place of the profile and region.
- The starship AWS module now reads whatever the hat's own config declares
  rather than `$AWS_PROFILE`, so a hat with an empty config shows nothing until
  you configure it inside that hat.

- **`profile` is now `hat`, everywhere.** The shell function, the subcommand and
  the config all use one word for the thing you put on. `profile <name>` becomes
  `hat <name>`, `hats profile ...` becomes `hats hat ...`, and the wizard's
  answer keys go from `profile.1.*` to `hat.1.*`. AWS profiles keep their own
  name throughout: they are a different thing that happened to share a word.
- **Breaking: `~/.hats/config.yaml` keys.** `profiles:` is now `hats:`, and the
  metadata block that used to be `hats:` is now `meta:`, since the collection
  had the better claim on the name. `default_profile` is `default_hat`. There is
  no migration: rename the three keys by hand.
- **Breaking: `HATS_PROFILE` is now `HATS_HAT`,** and the `DEV_PROFILE`
  compatibility alias is gone rather than renamed. The starship prompt reads
  `HATS_HAT`, and `~/.ssh/config.d/*.conf` rules that match on `$DEV_PROFILE`
  need updating to `$HATS_HAT`.
- `hats doctor` labels the resolve check `resolve` rather than reusing `hats`,
  which now names both the tool and the things it switches between.


## 0.2.0 — 2026-09-09

Packages become bundles you can pick from, and YubiKey enrolment actually works
end to end.

### Added

- **Package bundles.** The single flat `Brewfile` is now four files under
  `brew/`: `core`, plus `devops`, `pentest` and `dev`. Every bundle leads with
  `core`, so no machine ever gets a role set without the base set, and
  `hats brew install devops` means core + devops. `full` remains the default and
  means what the old Brewfile meant.
- **`hats brew <action> [bundle]`.** Install, check, clean up or dump against a
  named bundle. `brew bundle` takes a single `--file`, so hats concatenates the
  bundle into one temporary Brewfile before shelling out — which matters most
  for `cleanup`, since brew pointed at part of the set would offer to uninstall
  everything in the rest.
- **`HATS_BREW_BUNDLE`** selects the bundle the `brew-bundle` hook installs on
  apply, defaulting to `full`. The hook composes for itself rather than calling
  `hats brew`, because it also has to filter macOS-only entries on Linux.
- **Prefix history search.** Up and Down now search history for entries starting
  with what has been typed, the oh-my-zsh behaviour: with a prefix typed, the
  first Up lands on the line zsh-autosuggestions is already showing in grey.
  `HIST_FIND_NO_DUPS` stops the cycle offering the same line twice.

### Changed

- The Homebrew formula now declares `age-plugin-yubikey` as a dependency. hats
  drives it as a subprocess and the age crate resolves it on PATH at call time,
  so it has to be present rather than merely suggested.
- `hats brew dump` writes `Brewfile.new` beside the repo rather than next to a
  bundle file. A dump is a snapshot of the machine, not of a bundle, so it is
  for a human to diff and split by hand.
- The `brew-bundle` hook's `onchange` inputs list the four bundle files
  individually rather than the `brew/` directory: `onchange` hashes each input
  with `fs::read`, which fails on a directory and hashes as `<missing>`, so a
  directory input would never register a change.
- The README leads with the problem — where each tool keeps its answer and how
  far that reaches — and gains a TL;DR, a five-step quick start and an annotated
  profile example. The full command list moved behind a fold so the five
  commands you use daily are the ones you see.

### Fixed

- **`hats secrets enrol-yubikey` could not prompt for the PIV PIN.** Capturing
  the plugin's output left the prompt talking to a pipe, and it failed with "not
  a terminal". `--generate` now runs with our stdio attached, and the identity is
  read back afterwards with a second, non-interactive `--identity` call against
  the same slot.
- **The default PIV slot was wrong.** `age-plugin-yubikey` numbers the retired
  slots 1-20 rather than by their hex ids, so the documented default of `82` was
  never a slot it would accept. The default is now `1`.
- A stray `_pigeonhole` completion fragment in `.zshrc` ran on every shell start
  for a tool that is no longer installed.

## 0.1.0 — 2026-09-09

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
