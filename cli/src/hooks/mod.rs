//! Hooks: the scripts that run around an apply.
//!
//! These replace chezmoi's `run_once_` / `run_onchange_` naming convention. The
//! trigger is declared in `hats.yaml` rather than encoded in a filename, and an
//! `onchange` hook names its inputs explicitly instead of embedding a
//! `sha256sum` comment in the script. That means the hook script itself can be
//! an input, so editing the script re-runs it, which the old scheme missed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::repo::{HookSpec, Phase, SimpleTrigger, Trigger};
use crate::engine::state::{State, hash_paths};
use crate::platform::Platform;

/// Why a hook is going to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// A `once` hook that has never run.
    FirstRun,
    /// An `onchange` hook whose inputs moved.
    InputsChanged { from: Option<String>, to: String },
    /// An `always` hook.
    EveryApply,
    /// `hats hooks run --force`.
    Forced,
}

impl Reason {
    pub fn describe(&self) -> String {
        match self {
            Reason::FirstRun => "never run".into(),
            Reason::InputsChanged { from, to } => match from {
                Some(f) => format!("inputs changed ({} → {})", short(f), short(to)),
                None => "inputs not yet recorded".into(),
            },
            Reason::EveryApply => "runs on every apply".into(),
            Reason::Forced => "forced".into(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Reason::FirstRun => "once",
            Reason::InputsChanged { .. } => "onchange",
            Reason::EveryApply => "always",
            Reason::Forced => "forced",
        }
    }
}

fn short(h: &str) -> String {
    h.chars().take(7).collect()
}

/// A hook that is due to run.
#[derive(Debug, Clone)]
pub struct HookPlan {
    pub name: String,
    pub phase: Phase,
    pub script: PathBuf,
    pub reason: Reason,
    /// Hash to record on success, for `onchange` hooks.
    pub hash: Option<String>,
}

/// Decide which hooks are due.
///
/// Hooks whose platform condition does not match are dropped here rather than
/// running and exiting early, so a Linux plan simply never mentions the Dock.
pub fn due(
    specs: &[HookSpec],
    repo_dir: &Path,
    platform: &Platform,
    state: &State,
    force: Option<&str>,
) -> Vec<HookPlan> {
    let mut out = Vec::new();
    for spec in specs {
        if !spec.condition.matches(platform) {
            continue;
        }
        let forced = force == Some(spec.name.as_str());

        let (reason, hash) = match &spec.trigger {
            Trigger::Simple(SimpleTrigger::Always) => (Some(Reason::EveryApply), None),
            Trigger::Simple(SimpleTrigger::Once) => {
                let ran = state.hook(&spec.name).is_some();
                ((!ran).then_some(Reason::FirstRun), None)
            }
            Trigger::OnChange { onchange } => {
                let paths: Vec<PathBuf> = onchange.iter().map(|p| repo_dir.join(p)).collect();
                let now = hash_paths(&paths);
                let before = state.hook(&spec.name).and_then(|h| h.hash.clone());
                let changed = before.as_deref() != Some(now.as_str());
                (
                    changed.then(|| Reason::InputsChanged {
                        from: before,
                        to: now.clone(),
                    }),
                    Some(now),
                )
            }
        };

        // A forced run happens whatever the trigger says, but still records the
        // right hash so the next ordinary run is a no-op.
        let reason = if forced { Some(Reason::Forced) } else { reason };
        let Some(reason) = reason else { continue };

        out.push(HookPlan {
            name: spec.name.clone(),
            phase: spec.phase,
            script: repo_dir.join(&spec.script),
            reason,
            hash,
        });
    }
    out
}

/// Environment every hook receives. Hooks are plain `sh`, so this is how they
/// learn about the machine instead of being templated.
pub fn environment(
    platform: &Platform,
    repo_dir: &Path,
    files_dir: &Path,
    hats_home: &Path,
) -> Vec<(String, String)> {
    vec![
        ("HATS_HOME".into(), hats_home.to_string_lossy().into_owned()),
        ("HATS_REPO".into(), repo_dir.to_string_lossy().into_owned()),
        (
            "HATS_FILES".into(),
            files_dir.to_string_lossy().into_owned(),
        ),
        ("HATS_OS".into(), platform.os.to_string()),
        ("HATS_ARCH".into(), platform.arch.to_string()),
        (
            "HATS_BREW_PREFIX".into(),
            platform.brew_prefix.to_string_lossy().into_owned(),
        ),
        (
            "HATS_VERSION".into(),
            crate::repo::BINARY_VERSION.to_string(),
        ),
    ]
}

/// Run one hook, streaming its output with a prefix.
///
/// stdin is closed: a hook that tries to prompt would otherwise hang an
/// otherwise unattended apply.
pub fn run(hook: &HookPlan, repo_dir: &Path, env: &[(String, String)]) -> Result<()> {
    if !hook.script.exists() {
        anyhow::bail!(
            "hook `{}` has no script at {}",
            hook.name,
            hook.script.display()
        );
    }

    let mut cmd = std::process::Command::new("sh");
    cmd.arg(&hook.script)
        .current_dir(repo_dir)
        .stdin(std::process::Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }

    let status = cmd
        .status()
        .with_context(|| format!("running hook `{}`", hook.name))?;
    if !status.success() {
        anyhow::bail!(
            "hook `{}` failed with {}. Files already written are left in place; \
             re-run `hats apply` to continue.",
            hook.name,
            status
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, Os};

    fn platform(os: Os) -> Platform {
        Platform {
            os,
            arch: Arch::Arm64,
            home: PathBuf::from("/home/t"),
            user: "t".into(),
            hostname: "h".into(),
            brew_prefix: PathBuf::from("/opt/homebrew"),
        }
    }

    fn specs(yaml: &str) -> Vec<HookSpec> {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    const ALL: &str = r#"
- { name: once-hook, phase: after, trigger: once, script: hooks/a.sh }
- { name: always-hook, phase: after, trigger: always, script: hooks/b.sh }
- { name: change-hook, phase: after, trigger: { onchange: [Brewfile] }, script: hooks/c.sh }
- { name: mac-hook, phase: after, trigger: once, script: hooks/d.sh, os: [darwin] }
"#;

    fn repo_with_brewfile() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Brewfile"), "brew 'jq'\n").unwrap();
        dir
    }

    fn names(plans: &[HookPlan]) -> Vec<&str> {
        plans.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn on_a_fresh_machine_every_matching_hook_is_due() {
        let repo = repo_with_brewfile();
        let plans = due(
            &specs(ALL),
            repo.path(),
            &platform(Os::Darwin),
            &State::default(),
            None,
        );
        assert_eq!(
            names(&plans),
            vec!["once-hook", "always-hook", "change-hook", "mac-hook"]
        );
    }

    #[test]
    fn a_once_hook_does_not_run_twice() {
        let repo = repo_with_brewfile();
        let mut state = State::default();
        state.record_hook("once-hook", None);
        let plans = due(
            &specs(ALL),
            repo.path(),
            &platform(Os::Darwin),
            &state,
            None,
        );
        assert!(!names(&plans).contains(&"once-hook"));
    }

    #[test]
    fn an_always_hook_runs_every_time() {
        let repo = repo_with_brewfile();
        let mut state = State::default();
        state.record_hook("always-hook", None);
        let plans = due(
            &specs(ALL),
            repo.path(),
            &platform(Os::Darwin),
            &state,
            None,
        );
        assert!(names(&plans).contains(&"always-hook"));
    }

    #[test]
    fn an_onchange_hook_settles_then_re_runs_when_its_input_changes() {
        let repo = repo_with_brewfile();
        let s = specs(ALL);

        // First run records the hash.
        let mut state = State::default();
        let first = due(&s, repo.path(), &platform(Os::Darwin), &state, None);
        let change = first.iter().find(|p| p.name == "change-hook").unwrap();
        state.record_hook("change-hook", change.hash.clone());

        // Unchanged: not due.
        let second = due(&s, repo.path(), &platform(Os::Darwin), &state, None);
        assert!(!names(&second).contains(&"change-hook"));

        // Edited: due again, and the reason names both hashes.
        std::fs::write(repo.path().join("Brewfile"), "brew 'jq'\nbrew 'fd'\n").unwrap();
        let third = due(&s, repo.path(), &platform(Os::Darwin), &state, None);
        let plan = third.iter().find(|p| p.name == "change-hook").unwrap();
        assert!(matches!(
            plan.reason,
            Reason::InputsChanged { from: Some(_), .. }
        ));
        assert!(plan.reason.describe().contains("→"));
    }

    #[test]
    fn a_platform_gated_hook_is_absent_off_that_platform() {
        let repo = repo_with_brewfile();
        let plans = due(
            &specs(ALL),
            repo.path(),
            &platform(Os::Linux),
            &State::default(),
            None,
        );
        assert!(!names(&plans).contains(&"mac-hook"));
    }

    #[test]
    fn forcing_runs_a_settled_hook_and_still_records_the_hash() {
        let repo = repo_with_brewfile();
        let mut state = State::default();
        state.record_hook("once-hook", None);

        let plans = due(
            &specs(ALL),
            repo.path(),
            &platform(Os::Darwin),
            &state,
            Some("once-hook"),
        );
        let forced = plans.iter().find(|p| p.name == "once-hook").unwrap();
        assert_eq!(forced.reason, Reason::Forced);
    }

    #[test]
    fn the_environment_tells_a_hook_about_the_machine() {
        let env = environment(
            &platform(Os::Darwin),
            Path::new("/repo"),
            Path::new("/repo/files"),
            Path::new("/home/t/.hats"),
        );
        let map: std::collections::BTreeMap<_, _> = env.into_iter().collect();
        assert_eq!(map["HATS_OS"], "darwin");
        assert_eq!(map["HATS_ARCH"], "arm64");
        assert_eq!(map["HATS_BREW_PREFIX"], "/opt/homebrew");
        assert_eq!(map["HATS_REPO"], "/repo");
    }

    #[test]
    fn a_hook_runs_with_the_environment_and_cwd_set() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("hooks")).unwrap();
        let out = repo.path().join("out.txt");
        std::fs::write(
            repo.path().join("hooks/probe.sh"),
            format!(
                "#!/bin/sh\nprintf '%s|%s\\n' \"$HATS_OS\" \"$(pwd)\" > {}\n",
                out.display()
            ),
        )
        .unwrap();

        let hook = HookPlan {
            name: "probe".into(),
            phase: Phase::After,
            script: repo.path().join("hooks/probe.sh"),
            reason: Reason::FirstRun,
            hash: None,
        };
        let env = environment(
            &platform(Os::Darwin),
            repo.path(),
            &repo.path().join("files"),
            Path::new("/home/t/.hats"),
        );
        run(&hook, repo.path(), &env).unwrap();

        let written = std::fs::read_to_string(&out).unwrap();
        assert!(written.starts_with("darwin|"), "{written}");
    }

    #[test]
    fn a_failing_hook_reports_which_one_and_what_to_do() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("hooks")).unwrap();
        std::fs::write(repo.path().join("hooks/bad.sh"), "#!/bin/sh\nexit 3\n").unwrap();

        let hook = HookPlan {
            name: "bad".into(),
            phase: Phase::After,
            script: repo.path().join("hooks/bad.sh"),
            reason: Reason::FirstRun,
            hash: None,
        };
        let err = run(&hook, repo.path(), &[]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("`bad`"), "{msg}");
        assert!(msg.contains("hats apply"), "{msg}");
    }

    #[test]
    fn a_missing_script_is_reported_rather_than_silently_skipped() {
        let repo = tempfile::tempdir().unwrap();
        let hook = HookPlan {
            name: "ghost".into(),
            phase: Phase::After,
            script: repo.path().join("hooks/absent.sh"),
            reason: Reason::FirstRun,
            hash: None,
        };
        assert!(
            run(&hook, repo.path(), &[])
                .unwrap_err()
                .to_string()
                .contains("ghost")
        );
    }
}
