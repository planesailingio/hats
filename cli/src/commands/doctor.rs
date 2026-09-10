//! `hats doctor` — is this machine ready, and if not, what should I run?
//!
//! Each check reports a status and, when it fails, the one command that fixes
//! it. Exit code is non-zero only for problems that block hats, not for
//! optional tooling that is merely absent.

use anyhow::Result;

use crate::app::App;
use crate::cli::DoctorArgs;
use crate::config::Config;
use crate::platform::Os;
use crate::repo::{BINARY_VERSION, VersionStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    /// Works, but something optional is missing.
    Warn,
    /// hats cannot do its job until this is fixed.
    Fail,
}

impl Level {
    fn mark(self) -> &'static str {
        match self {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        }
    }
}

#[derive(Debug)]
pub struct Check {
    pub name: String,
    pub level: Level,
    pub detail: String,
    /// The command that fixes it, if there is one.
    pub fix: Option<String>,
}

impl Check {
    fn ok(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            level: Level::Ok,
            detail: detail.into(),
            fix: None,
        }
    }
    fn warn(name: &str, detail: impl Into<String>, fix: Option<&str>) -> Self {
        Self {
            name: name.into(),
            level: Level::Warn,
            detail: detail.into(),
            fix: fix.map(str::to_owned),
        }
    }
    fn fail(name: &str, detail: impl Into<String>, fix: Option<&str>) -> Self {
        Self {
            name: name.into(),
            level: Level::Fail,
            detail: detail.into(),
            fix: fix.map(str::to_owned),
        }
    }
}

/// Tools hats or its hooks call. `required` ones are checked as failures.
const TOOLS: &[(&str, bool, &str)] = &[
    ("git", true, "cloning and updating the dotfiles repo"),
    ("zsh", false, "the shell the hat switcher targets"),
    ("brew", false, "the Brewfile hook"),
    (
        "fzf",
        false,
        "the hat picker when `hat` is run with no name",
    ),
    ("kubectl", false, "per-hat kube context switching"),
    (
        // Installed as a Homebrew dependency, so an absence here means a source
        // build or a broken install. Still not fatal: it is only needed to
        // unseal the credential envelope.
        "age-plugin-yubikey",
        false,
        "unsealing the YubiKey-protected credential envelope",
    ),
];

pub fn run(app: &mut App, args: &DoctorArgs) -> Result<()> {
    let checks = collect(app);

    if args.json {
        let items: Vec<serde_json::Value> = checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "level": match c.level {
                        Level::Ok => "ok",
                        Level::Warn => "warn",
                        Level::Fail => "fail",
                    },
                    "detail": c.detail,
                    "fix": c.fix,
                })
            })
            .collect();
        app.ui.say(serde_json::to_string(&items)?);
    } else {
        for c in &checks {
            app.ui
                .say(format!("{}  {:<12} {}", c.level.mark(), c.name, c.detail));
            if let Some(fix) = &c.fix {
                app.ui.say(format!("      {:<12} → {fix}", ""));
            }
        }
    }

    if checks.iter().any(|c| c.level == Level::Fail) {
        std::process::exit(1);
    }
    Ok(())
}

/// Run every check. Separated from printing so tests can assert on results.
pub fn collect(app: &App) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.push(Check::ok("hats", format!("v{BINARY_VERSION}")));

    // Platform.
    match app.platform() {
        Ok(p) => checks.push(Check::ok(
            "platform",
            format!(
                "{} {} (brew prefix {})",
                p.os,
                p.arch,
                p.brew_prefix.display()
            ),
        )),
        Err(e) => checks.push(Check::fail("platform", e.to_string(), None)),
    }

    // Tools.
    for (tool, required, why) in TOOLS {
        match which::which(tool) {
            Ok(path) => checks.push(Check::ok(tool, path.display().to_string())),
            Err(_) if *required => checks.push(Check::fail(
                tool,
                format!("not found; needed for {why}"),
                Some(&format!("brew install {tool}")),
            )),
            Err(_) => checks.push(Check::warn(
                tool,
                format!("not found; {why} will not work"),
                Some(&format!("brew install {tool}")),
            )),
        }
    }

    // Home directory and repo.
    if app.paths.root.is_dir() {
        checks.push(Check::ok("hats home", app.paths.root.display().to_string()));
    } else {
        checks.push(Check::warn(
            "hats home",
            format!("{} does not exist", app.paths.root.display()),
            Some("hats init"),
        ));
    }

    let repo = app.repo();
    match repo.status() {
        Ok(VersionStatus::Missing) => checks.push(Check::fail(
            "repo",
            format!("no clone at {}", repo.path.display()),
            Some("hats init"),
        )),
        Ok(status @ VersionStatus::Match { .. }) => {
            checks.push(Check::ok("repo", status.summary()))
        }
        Ok(status @ VersionStatus::Untagged { .. }) => {
            checks.push(Check::warn("repo", status.summary(), None))
        }
        Ok(status @ VersionStatus::RepoBehind { .. }) => {
            checks.push(Check::warn("repo", status.summary(), Some("hats update")))
        }
        Ok(status @ VersionStatus::RepoAhead { .. }) => checks.push(Check::warn(
            "repo",
            status.summary(),
            Some("brew upgrade hats"),
        )),
        Err(e) => checks.push(Check::fail("repo", e.to_string(), Some("hats init"))),
    }

    // Configuration.
    if !app.paths.is_initialised() {
        checks.push(Check::fail(
            "config",
            format!("{} is missing", app.paths.config.display()),
            Some("hats init"),
        ));
        return checks;
    }

    match Config::load(&app.paths) {
        Ok(cfg) => {
            let names = cfg.hat_names();
            checks.push(Check::ok(
                "config",
                format!(
                    "{} hat{}: {}",
                    names.len(),
                    if names.len() == 1 { "" } else { "s" },
                    names.join(", ")
                ),
            ));

            let problems = cfg.problems();
            if problems.is_empty() {
                checks.push(Check::ok("resolve", "hats resolve cleanly"));
            } else {
                for p in problems {
                    checks.push(Check::fail("resolve", p, Some("hats hat list")));
                }
            }

            if cfg.group_enabled("ssh") {
                checks.push(ssh_check(app));
            }

            // Secrets file, if a provider is configured.
            if cfg.local.secrets.provider != crate::config::local::ProviderKind::None {
                if app.paths.secrets.is_file() {
                    checks.push(Check::ok(
                        "secrets",
                        format!("{} present", app.paths.secrets.display()),
                    ));
                } else {
                    checks.push(Check::warn(
                        "secrets",
                        "no secrets fetched yet; templates render with empty values",
                        Some("hats secrets fetch"),
                    ));
                }
            }
        }
        Err(e) => checks.push(Check::fail("config", e.to_string(), Some("hats init"))),
    }

    checks
}

/// OpenSSH expands environment variables in `Include` from 9.9 on.
const SSH_INCLUDE_ENV: (u32, u32) = (9, 9);

/// The ssh skeleton includes `~/.ssh/config.d/${HATS_HAT}.conf`. An older ssh
/// reads that path literally, matches nothing and never says so, so a hat's
/// hosts and keys would silently not apply.
fn ssh_check(app: &App) -> Check {
    let Ok(out) = std::process::Command::new("ssh").arg("-V").output() else {
        return Check::warn("ssh", "not found; per-hat ssh config will not work", None);
    };
    // `ssh -V` prints its banner to stderr.
    let banner = String::from_utf8_lossy(&out.stderr).trim().to_string();
    match openssh_version(&banner) {
        Some(v) if v >= SSH_INCLUDE_ENV => Check::ok("ssh", banner),
        Some((major, minor)) => {
            // Homebrew's openssh lacks Apple's UseKeychain, which the skeleton
            // sets on macOS, so only suggest it on Linux.
            let linux = app.platform().is_ok_and(|p| matches!(p.os, Os::Linux));
            Check::warn(
                "ssh",
                format!(
                    "OpenSSH {major}.{minor} cannot expand ${{HATS_HAT}} in Include; \
                     ~/.ssh/config.d/<hat>.conf needs 9.9+"
                ),
                linux.then_some("brew install openssh"),
            )
        }
        None => Check::warn("ssh", format!("unrecognised version `{banner}`"), None),
    }
}

/// `OpenSSH_10.2p1, LibreSSL 3.3.6` → `(10, 2)`.
fn openssh_version(banner: &str) -> Option<(u32, u32)> {
    let rest = banner.strip_prefix("OpenSSH_")?;
    let mut parts = rest.split(|c: char| !c.is_ascii_digit());
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::GlobalOpts;

    fn app_in(dir: &std::path::Path) -> App {
        App::new(&GlobalOpts {
            hats_home: Some(dir.to_path_buf()),
            non_interactive: true,
            answers: None,
            no_color: true,
            allow_mismatch: false,
            verbose: 0,
        })
        .unwrap()
    }

    #[test]
    fn an_uninitialised_machine_fails_on_repo_and_config() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_in(dir.path());
        let checks = collect(&app);

        let repo = checks.iter().find(|c| c.name == "repo").unwrap();
        assert_eq!(repo.level, Level::Fail);
        assert_eq!(repo.fix.as_deref(), Some("hats init"));

        let config = checks.iter().find(|c| c.name == "config").unwrap();
        assert_eq!(config.level, Level::Fail);
    }

    #[test]
    fn git_is_checked_as_required() {
        let dir = tempfile::tempdir().unwrap();
        let checks = collect(&app_in(dir.path()));
        let git = checks.iter().find(|c| c.name == "git").unwrap();
        // Tests cannot run without git on PATH, so this must pass.
        assert_eq!(git.level, Level::Ok, "{}", git.detail);
    }

    #[test]
    fn optional_tooling_never_fails_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let checks = collect(&app_in(dir.path()));
        for name in ["fzf", "kubectl", "age-plugin-yubikey", "brew", "zsh"] {
            let c = checks.iter().find(|c| c.name == name).unwrap();
            assert_ne!(c.level, Level::Fail, "{name} must not be a hard failure");
        }
    }

    #[test]
    fn openssh_versions_parse_from_real_banners() {
        assert_eq!(
            openssh_version("OpenSSH_10.2p1, LibreSSL 3.3.6"),
            Some((10, 2))
        );
        assert_eq!(
            openssh_version("OpenSSH_9.6p1 Ubuntu-3ubuntu13.5, OpenSSL 3.0.13 30 Jan 2024"),
            Some((9, 6))
        );
        assert_eq!(openssh_version("OpenSSH_for_Windows_9.5p1"), None);
        assert_eq!(openssh_version(""), None);
    }

    /// Compared as numbers, not strings: 10.0 is new enough, 9.6 is not.
    #[test]
    fn include_expansion_needs_9_9_or_later() {
        assert!((9, 6) < SSH_INCLUDE_ENV);
        assert!((9, 9) >= SSH_INCLUDE_ENV);
        assert!((10, 0) >= SSH_INCLUDE_ENV);
    }

    #[test]
    fn levels_render_a_fixed_width_marker() {
        assert_eq!(Level::Ok.mark().len(), 4);
        assert_eq!(Level::Warn.mark().len(), 4);
        assert_eq!(Level::Fail.mark().len(), 4);
    }
}
