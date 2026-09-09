# Releasing hats

Releases are cut from a `vX.Y.Z` tag on `main`. The `Release` workflow
(`.github/workflows/release.yml`) builds the archives, publishes a GitHub
release and, for final versions, pushes the Homebrew formula to
`planesailingio/homebrew-tools`.

The binary and the dotfiles content ship from **one tag**. That is the whole
point: `hats update` checks out `v{binary version}`, so a given `hats` can only
ever drive the dotfiles it was released with. A `verify` job fails the release
if `cli/Cargo.toml` and the tag disagree.

## Cutting a release

1. Bump `version` in `cli/Cargo.toml` and run `cargo build` so `Cargo.lock`
   follows.
2. Move the `Unreleased` entries in `CHANGELOG.md` under a new `X.Y.Z` heading
   with the date.
3. Commit, make sure CI is green on `main`, then tag and push:

   ```sh
   git tag -a vX.Y.Z -m "hats X.Y.Z"
   git push origin vX.Y.Z
   ```

Tags containing `-` (`v0.2.0-rc1`) are published as prereleases and skip the
tap.

## What the workflow does

- `verify` — asserts `cli/Cargo.toml` version equals the tag minus `v`.
- `build` — one job per target (`x86_64-apple-darwin`,
  `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`,
  `aarch64-unknown-linux-gnu`), each on a native runner except the aarch64
  Linux leg, which cross-links with `gcc-aarch64-linux-gnu`. Each produces
  `hats_<version>_<target>.tar.gz` containing `<name>/bin/hats`, the three
  shell completions, `README.md` and `LICENSE`, plus a `.sha256` sidecar.
- `release` — collects the archives and creates the GitHub release with
  `softprops/action-gh-release`; `prerelease` is set when the tag contains `-`.
- `tap` — `needs: release`, skipped for prereleases. Runs
  `HATS_TAP_CONFIRM=1 scripts/update-tap.sh <version>`, which downloads the four
  `.sha256` sidecars from the release, renders `Formula/hats.rb`, clones the
  tap, commits `hats <version>` and pushes.

There is no GoReleaser here. It is used by the org's Go projects; every Rust
tool (moss, twig, gannet) hand-rolls the matrix, and hats follows that.

## The tap GitHub App

The `tap` job pushes to a different repository, which the default
`GITHUB_TOKEN` cannot do: it is scoped to the repository the workflow runs in,
and no setting extends it. The job instead mints a one-hour installation token
for the `all-ci-workflows` GitHub App using `actions/create-github-app-token`,
and passes it to the script as `GH_TOKEN`.

The app is `all-ci-workflows`, the same one `moss` and `twig` use. Each
repository carries its own copy of the credentials:

- **Variable** `TAP_APP_ID` — the app's client ID, `Iv23lio9RIHPFxKIbCCU`.
- **Secret** `TAP_APP_PRIVATE_KEY` — the full `.pem` contents, BEGIN and END
  lines included.

Both are already set on this repository. Set them elsewhere with:

```sh
gh variable set TAP_APP_ID --repo planesailingio/<repo> --body Iv23lio9RIHPFxKIbCCU
gh secret set TAP_APP_PRIVATE_KEY --repo planesailingio/<repo> < path/to/key.pem
```

The app must stay installed on `homebrew-tools`. Creating an app does not
install it, and without the installation every tap push fails with `Not Found`
on the installation lookup.

Commits pushed this way are authored by the app's bot user.

## Prerequisite: this repository must live in the org

The tap job reads **org-level** values from `planesailingio`. A workflow running
under a personal account cannot read them, so the repository has to be
`planesailingio/hats` before the first tag. GitHub leaves a redirect behind
after a transfer, so existing clones and the bootstrap URL keep working, but
update `bootstrap.sh`, `README.md` and the docs to the new path anyway.

## Dry run before the first real release

1. Push a prerelease tag, e.g. `v0.0.1-rc1`. Confirm the release shows four
   archives with four sidecars, is marked as a prerelease, and the `tap` job was
   skipped.
2. Render the formula locally without pushing (answer `N` at the prompt):

   ```sh
   scripts/update-tap.sh 0.0.1-rc1
   ```

   Check `Formula/hats.rb` has four real SHA-256 digests and the right URLs,
   then delete it.
3. Push the final tag and watch the `tap` job. Verify with:

   ```sh
   brew update && brew install planesailingio/tools/hats && hats version
   ```
