//! `hats version` — the binary, the repo, and whether they agree.

use anyhow::Result;

use crate::app::App;
use crate::cli::VersionArgs;
use crate::repo::{BINARY_SHA, BINARY_VERSION, VersionStatus};

pub fn run(app: &mut App, args: &VersionArgs) -> Result<()> {
    let repo = app.repo();
    let status = repo.status().unwrap_or(VersionStatus::Missing);
    let repo_tag = match &status {
        VersionStatus::Match { tag } => Some(tag.clone()),
        VersionStatus::RepoBehind { repo, .. } => repo.clone(),
        VersionStatus::RepoAhead { repo, .. } => Some(repo.clone()),
        _ => None,
    };
    let head = repo.head_sha().ok();

    if args.json {
        app.ui.say(serde_json::to_string(&serde_json::json!({
            "version": BINARY_VERSION,
            "sha": BINARY_SHA,
            "repo_path": repo.path.display().to_string(),
            "repo_tag": repo_tag,
            "repo_head": head,
            "in_step": status.is_ok(),
        }))?);
        return Ok(());
    }

    app.ui.say(format!("hats {BINARY_VERSION} ({BINARY_SHA})"));
    app.ui.say(format!("repo {}", repo.path.display()));
    if let Some(tag) = &repo_tag {
        app.ui.say(format!(
            "     {tag}{}",
            head.map(|h| format!(" ({h})")).unwrap_or_default()
        ));
    }
    app.ui.say(status.summary());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_binary_version_is_a_real_semver() {
        assert!(semver::Version::parse(BINARY_VERSION).is_ok());
    }
}
