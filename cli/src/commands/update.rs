//! `hats update` — move the dotfiles clone to the tag matching this binary.
//!
//! There is no version argument. The binary and the repo ship from one tag, so
//! the only correct target is `v{CARGO_PKG_VERSION}`. The usual sequence is
//! `brew upgrade hats` followed by `hats update`.

use anyhow::{Context, Result};

use crate::app::App;
use crate::cli::UpdateArgs;
use crate::repo::{BINARY_VERSION, VersionStatus};

/// Exit code carried out of `--check`, mapped by main.
pub struct CheckExit(pub i32);

pub fn run(app: &mut App, args: &UpdateArgs) -> Result<Option<CheckExit>> {
    let repo = app.repo();

    if args.check {
        // Fetch first so "behind" reflects the remote, not a stale clone.
        if repo.exists()
            && let Err(e) = repo.fetch_tags()
        {
            app.ui.detail(format!("could not fetch tags: {e}"));
        }
        let status = repo.status()?;
        app.ui.say(format!("hats      v{BINARY_VERSION}"));
        match &status {
            VersionStatus::Missing => app.ui.say("repo      (not cloned)"),
            VersionStatus::Untagged { .. } => app.ui.say("repo      (untagged)"),
            VersionStatus::Match { tag } => app.ui.say(format!("repo      {tag}")),
            VersionStatus::RepoBehind { repo: r, .. } => {
                app.ui.say(format!(
                    "repo      {}",
                    r.as_deref().unwrap_or("(untagged)")
                ));
            }
            VersionStatus::RepoAhead { repo: r, .. } => app.ui.say(format!("repo      {r}")),
        }
        app.ui.say(status.summary());
        return Ok(Some(CheckExit(status.exit_code())));
    }

    let tag = repo
        .update_to_binary_version(args.force)
        .context("updating the dotfiles repo")?;
    app.ui.ok(format!("repo is now at {tag}"));
    app.ui
        .say("Run `hats plan` to see what that changes in your home directory.");
    Ok(None)
}

/// Guard for commands that read the repo. Returns false when the caller should
/// stop.
///
/// The common skew is a Homebrew upgrade that moved the binary ahead of the
/// clone, which one command fixes, so offer to run it rather than just
/// complaining.
pub fn ensure_in_step(app: &mut App) -> Result<bool> {
    if app.allow_mismatch {
        return Ok(true);
    }
    let repo = app.repo();
    let status = repo.status()?;
    if status.is_ok() {
        return Ok(true);
    }

    match status {
        VersionStatus::RepoBehind { .. } => {
            app.ui.warn(status.summary());
            let fix = app.ui.prompter.confirm(
                "update.now",
                "Update the dotfiles repo to match this hats now?",
                true,
            )?;
            if fix {
                let tag = repo.update_to_binary_version(false)?;
                app.ui.ok(format!("repo is now at {tag}"));
                return Ok(true);
            }
            app.ui
                .warn("continuing would use a repo that does not match this binary");
            Ok(false)
        }
        // A development clone is a deliberate state; warn once and carry on.
        VersionStatus::Untagged { .. } => {
            app.ui.detail(status.summary());
            Ok(true)
        }
        other => {
            app.ui.warn(other.summary());
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_exit_codes_follow_the_documented_contract() {
        assert_eq!(VersionStatus::Match { tag: "v1".into() }.exit_code(), 0);
        assert_eq!(
            VersionStatus::RepoBehind {
                binary: "1".into(),
                repo: None
            }
            .exit_code(),
            3
        );
        assert_eq!(
            VersionStatus::RepoAhead {
                binary: "1".into(),
                repo: "v2".into()
            }
            .exit_code(),
            4
        );
        assert_eq!(VersionStatus::Missing.exit_code(), 1);
    }
}
