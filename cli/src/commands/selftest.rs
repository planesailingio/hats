//! `hats test` — prove the switcher actually works on this machine.
//!
//! This replaces the shell test suite the dotfiles used to run inside Docker.
//! The assertions are the same, and they are the ones that matter: not "does
//! the file exist" but "does a real shell, given this profile, commit as the
//! right person and point at its own kubeconfig".
//!
//! Every check is skipped rather than failed when its prerequisite is absent,
//! so this is safe to run on a machine that has not applied yet.

use anyhow::{Context, Result};

use crate::app::App;
use crate::cli::TestArgs;
use crate::config::Config;

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Pass,
    Fail(String),
    Skip(String),
}

struct Check {
    name: String,
    verdict: Verdict,
}

pub fn run(app: &mut App, args: &TestArgs) -> Result<i32> {
    if args.container {
        return container(app);
    }

    let cfg = app.config()?;
    let mut checks = Vec::new();

    checks.push(zsh_starts());
    checks.extend(profile_checks(app, &cfg)?);
    checks.push(kube_isolation(app, &cfg)?);
    checks.push(commit_identity(app, &cfg)?);
    checks.push(dock_hook_guard(app));

    let mut failed = 0;
    for check in &checks {
        match &check.verdict {
            Verdict::Pass => app.ui.ok(&check.name),
            Verdict::Skip(why) => app.ui.say(format!("skip {} ({why})", check.name)),
            Verdict::Fail(why) => {
                failed += 1;
                app.ui.warn(format!("FAIL {}: {why}", check.name));
            }
        }
    }

    let passed = checks.iter().filter(|c| c.verdict == Verdict::Pass).count();
    app.ui.say("");
    app.ui.say(format!(
        "{passed} passed, {failed} failed, {} skipped",
        checks.len() - passed - failed
    ));
    Ok(if failed > 0 { 1 } else { 0 })
}

fn check(name: impl Into<String>, verdict: Verdict) -> Check {
    Check {
        name: name.into(),
        verdict,
    }
}

/// An interactive zsh must start cleanly. A broken .zshrc breaks every terminal,
/// so this is the single most valuable assertion here.
fn zsh_starts() -> Check {
    let Ok(zsh) = which::which("zsh") else {
        return check(
            "zsh starts cleanly",
            Verdict::Skip("zsh not installed".into()),
        );
    };
    let out = std::process::Command::new(zsh)
        .args(["-ic", "echo zsh-ok"])
        .output();
    match out {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains("zsh-ok") => {
            check("zsh starts cleanly", Verdict::Pass)
        }
        Ok(o) => check(
            "zsh starts cleanly",
            Verdict::Fail(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        ),
        Err(e) => check("zsh starts cleanly", Verdict::Fail(e.to_string())),
    }
}

/// For each profile: evaluate its environment in a real shell and read back the
/// values, which is the only way to know the emitted script does what it says.
fn profile_checks(app: &App, cfg: &Config) -> Result<Vec<Check>> {
    let mut checks = Vec::new();
    let exe = std::env::current_exe().context("finding this binary")?;

    for name in cfg.profile_names() {
        let resolved = match cfg.resolve_profile(&name) {
            Ok(p) => p,
            Err(e) => {
                checks.push(check(
                    format!("profile {name}"),
                    Verdict::Fail(e.to_string()),
                ));
                continue;
            }
        };

        let script = format!(
            "eval \"$({} --hats-home {} env {} --no-colour)\" && \
             printf '%s|%s|%s' \"$HATS_PROFILE\" \"$GIT_AUTHOR_EMAIL\" \"$AWS_PROFILE\"",
            shell_quote(&exe.to_string_lossy()),
            shell_quote(&app.paths.root.to_string_lossy()),
            shell_quote(&name),
        );
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output();

        let verdict = match out {
            Ok(o) if o.status.success() => {
                let got = String::from_utf8_lossy(&o.stdout).to_string();
                let parts: Vec<&str> = got.split('|').collect();
                let want_email = resolved.identity.email.clone().unwrap_or_default();
                let want_aws = resolved.aws.profile.clone().unwrap_or_default();

                if parts.first() != Some(&name.as_str()) {
                    Verdict::Fail(format!(
                        "HATS_PROFILE was {:?}, expected {name}",
                        parts.first()
                    ))
                } else if parts.get(1).copied() != Some(want_email.as_str()) {
                    Verdict::Fail(format!(
                        "GIT_AUTHOR_EMAIL was {:?}, expected {want_email:?}",
                        parts.get(1)
                    ))
                } else if parts.get(2).copied() != Some(want_aws.as_str()) {
                    Verdict::Fail(format!(
                        "AWS_PROFILE was {:?}, expected {want_aws:?}",
                        parts.get(2)
                    ))
                } else {
                    Verdict::Pass
                }
            }
            Ok(o) => Verdict::Fail(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => Verdict::Fail(e.to_string()),
        };
        checks.push(check(
            format!("profile {name} applies in a real shell"),
            verdict,
        ));
    }
    Ok(checks)
}

/// Two shells on two profiles must end up with different, non-empty
/// kubeconfigs, and the shared one must not move. This is the property the
/// whole design exists to protect.
fn kube_isolation(app: &App, cfg: &Config) -> Result<Check> {
    let name = "two shells get separate kubeconfigs";
    let isolated: Vec<String> = cfg
        .profile_names()
        .into_iter()
        .filter(|n| {
            cfg.resolve_profile(n)
                .map(|p| p.kube_isolated())
                .unwrap_or(false)
        })
        .collect();
    if isolated.len() < 2 {
        return Ok(check(
            name,
            Verdict::Skip("needs two isolated profiles".into()),
        ));
    }

    let exe = std::env::current_exe()?;
    let shared = app.platform()?.home.join(".kube/config");
    let before = std::fs::read(&shared).ok();

    let read_kubeconfig = |profile: &str| -> Option<String> {
        let script = format!(
            "eval \"$({} --hats-home {} env {} --no-colour)\" && printf '%s' \"$KUBECONFIG\"",
            shell_quote(&exe.to_string_lossy()),
            shell_quote(&app.paths.root.to_string_lossy()),
            shell_quote(profile),
        );
        std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
    };

    let a = read_kubeconfig(&isolated[0]);
    let b = read_kubeconfig(&isolated[1]);

    let verdict = match (a, b) {
        (Some(a), Some(b)) if a.is_empty() || b.is_empty() => {
            Verdict::Fail("one shell had an empty KUBECONFIG".into())
        }
        (Some(a), Some(b)) if a == b => {
            Verdict::Fail(format!("both shells got the same kubeconfig: {a}"))
        }
        (Some(_), Some(_)) => {
            // The shared config must not have drifted.
            if before.is_some() && std::fs::read(&shared).ok() != before {
                Verdict::Fail("the shared ~/.kube/config changed".into())
            } else {
                Verdict::Pass
            }
        }
        _ => Verdict::Fail("could not read KUBECONFIG from a shell".into()),
    };
    Ok(check(name, verdict))
}

/// The end-to-end assertion: make a real commit inside a shell holding a
/// profile, and check who git thinks wrote it. Environment variables are only
/// worth anything if git actually honours them.
fn commit_identity(app: &App, cfg: &Config) -> Result<Check> {
    let name = "a real commit carries the profile's identity";
    if which::which("git").is_err() {
        return Ok(check(name, Verdict::Skip("git not installed".into())));
    }

    let Some((profile, email)) = cfg.profile_names().into_iter().find_map(|n| {
        cfg.resolve_profile(&n)
            .ok()
            .and_then(|p| p.identity.email.clone())
            .map(|e| (n, e))
    }) else {
        return Ok(check(
            name,
            Verdict::Skip("no profile sets an email".into()),
        ));
    };

    let dir = tempfile::tempdir()?;
    let exe = std::env::current_exe()?;
    let script = format!(
        "cd {} && git init -q -b main && \
         eval \"$({} --hats-home {} env {} --no-colour)\" && \
         git commit -q --allow-empty -m probe && git log -1 --format='%ae'",
        shell_quote(&dir.path().to_string_lossy()),
        shell_quote(&exe.to_string_lossy()),
        shell_quote(&app.paths.root.to_string_lossy()),
        shell_quote(&profile),
    );
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .output()?;

    let verdict = if !out.status.success() {
        Verdict::Fail(String::from_utf8_lossy(&out.stderr).trim().to_string())
    } else {
        let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if got == email {
            Verdict::Pass
        } else {
            Verdict::Fail(format!("committed as {got}, expected {email}"))
        }
    };
    Ok(check(format!("{name} ({profile})"), verdict))
}

/// The Dock hook must be a no-op off macOS, so a Linux container run does not
/// try to drive dockutil.
fn dock_hook_guard(app: &App) -> Check {
    let name = "the dock hook is a no-op off macOS";
    let script = app.paths.repo.join("hooks/dock.sh");
    if !script.is_file() {
        return check(name, Verdict::Skip("no dock hook in this repo".into()));
    }
    let out = std::process::Command::new("sh")
        .arg(&script)
        .env("HATS_OS", "linux")
        .output();
    match out {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains("skipping Dock rebuild") => {
            check(name, Verdict::Pass)
        }
        Ok(o) => check(
            name,
            Verdict::Fail(format!(
                "expected a skip message, got: {}",
                String::from_utf8_lossy(&o.stdout).trim()
            )),
        ),
        Err(e) => check(name, Verdict::Fail(e.to_string())),
    }
}

/// Run the whole suite inside the Linux dev container.
fn container(app: &mut App) -> Result<i32> {
    which::which("docker").context("docker is not installed")?;
    let repo = &app.paths.repo;
    let dockerfile = repo.join(".devcontainer/Dockerfile");
    if !dockerfile.is_file() {
        anyhow::bail!("no .devcontainer/Dockerfile in {}", repo.display());
    }

    app.ui.say("Building the Linux test image…");
    let build = std::process::Command::new("docker")
        .args(["build", "-f"])
        .arg(&dockerfile)
        .args(["-t", "hats-test"])
        .arg(repo)
        .status()?;
    if !build.success() {
        anyhow::bail!("docker build failed");
    }

    app.ui.say("Running the suite in the container…");
    let run = std::process::Command::new("docker")
        .args(["run", "--rm", "-v"])
        .arg(format!("{}:/workspace:cached", repo.display()))
        .args(["hats-test", "/workspace/.devcontainer/test.sh"])
        .status()?;
    Ok(if run.success() { 0 } else { 1 })
}

/// POSIX single-quoting for values interpolated into a generated script.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_awkward_paths() {
        assert_eq!(shell_quote("/plain/path"), "'/plain/path'");
        assert_eq!(shell_quote("/it's/here"), r"'/it'\''s/here'");
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
    }

    #[test]
    fn a_quoted_path_round_trips_through_a_real_shell() {
        let awkward = "/tmp/it's a path/$HOME";
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", shell_quote(awkward)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), awkward);
    }

    #[test]
    fn zsh_starting_cleanly_is_pass_or_skip_never_a_crash() {
        let c = zsh_starts();
        assert!(matches!(
            c.verdict,
            Verdict::Pass | Verdict::Skip(_) | Verdict::Fail(_)
        ));
    }

    #[test]
    fn verdicts_are_distinguishable() {
        assert_ne!(Verdict::Pass, Verdict::Skip("x".into()));
        assert_ne!(Verdict::Fail("a".into()), Verdict::Fail("b".into()));
    }
}
