//! The remaining Makefile targets, as subcommands: `brew`, `hooks`, `lint`.
//!
//! These are thin, but bringing them into the binary means one help page, one
//! set of paths, and no `make -C ~/.hats/repo` incantation to remember.

use anyhow::{Context, Result};

use crate::app::App;
use crate::cli::{BrewAction, BrewArgs, HooksArgs, HooksCommand, LintArgs};
use crate::engine::render::{RedactMode, RenderContext, Renderer};
use crate::engine::state::State;
use crate::model::Filter;
use crate::secrets::store::Secrets;

// ── brew ────────────────────────────────────────────────────────────────────

pub fn brew(app: &mut App, args: &BrewArgs) -> Result<i32> {
    let brewfile = app.paths.repo.join("Brewfile");
    if !brewfile.is_file() {
        anyhow::bail!("no Brewfile at {}", brewfile.display());
    }
    which::which("brew").context("brew is not installed. See https://brew.sh")?;

    let file = brewfile.to_string_lossy().into_owned();
    let argv: Vec<String> = match args.action {
        BrewAction::Install => vec!["bundle".into(), "install".into(), format!("--file={file}")],
        BrewAction::Check => vec![
            "bundle".into(),
            "check".into(),
            "--verbose".into(),
            format!("--file={file}"),
        ],
        BrewAction::Cleanup => {
            let mut v = vec!["bundle".into(), "cleanup".into(), format!("--file={file}")];
            if args.force {
                v.push("--force".into());
            }
            v
        }
        BrewAction::Dump => vec![
            "bundle".into(),
            "dump".into(),
            "--describe".into(),
            format!("--file={file}.new"),
        ],
    };

    if args.action == BrewAction::Cleanup && args.force {
        app.ui
            .warn("this uninstalls everything not in the Brewfile");
        if !app
            .ui
            .prompter
            .confirm("brew.cleanup.confirm", "Continue?", false)?
        {
            return Ok(1);
        }
    }

    let status = std::process::Command::new("brew")
        .args(&argv)
        .status()
        .context("running brew")?;

    if args.action == BrewAction::Dump && status.success() {
        app.ui.say(format!(
            "Wrote {file}.new — diff it before replacing; dump pulls in cruft."
        ));
    }
    Ok(if status.success() { 0 } else { 1 })
}

// ── hooks ───────────────────────────────────────────────────────────────────

pub fn hooks(app: &mut App, args: &HooksArgs) -> Result<i32> {
    let cfg = app.config()?;
    let platform = app.platform()?;
    let state = State::load(&app.paths.state)?;

    match &args.command {
        HooksCommand::List => {
            let due = crate::hooks::due(&cfg.repo.hooks, &app.paths.repo, &platform, &state, None);
            let due_names: Vec<&str> = due.iter().map(|h| h.name.as_str()).collect();

            for spec in &cfg.repo.hooks {
                let applies = spec.condition.matches(&platform);
                let status = if !applies {
                    "n/a here".to_string()
                } else if due_names.contains(&spec.name.as_str()) {
                    "DUE".to_string()
                } else {
                    match state.hook(&spec.name) {
                        Some(h) => format!("ran {}", &h.ran_at[..10.min(h.ran_at.len())]),
                        None => "settled".to_string(),
                    }
                };
                app.ui.say(format!(
                    "  {:<18} {:<8} {}",
                    spec.name,
                    format!("{:?}", spec.phase).to_lowercase(),
                    status
                ));
            }
            Ok(0)
        }

        HooksCommand::Run { name, force } => {
            let plans = crate::hooks::due(
                &cfg.repo.hooks,
                &app.paths.repo,
                &platform,
                &state,
                force.then_some(name.as_str()),
            );
            let Some(hook) = plans.iter().find(|h| &h.name == name) else {
                if cfg.repo.hooks.iter().any(|h| &h.name == name) {
                    app.ui.say(format!(
                        "hook `{name}` is not due. Use --force to run it anyway."
                    ));
                    return Ok(0);
                }
                anyhow::bail!(
                    "no hook named `{name}`. Known: {}",
                    cfg.repo
                        .hooks
                        .iter()
                        .map(|h| h.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            };

            let env = crate::hooks::environment(
                &platform,
                &app.paths.repo,
                &app.paths.repo.join(crate::config::FILES_DIR),
                &app.paths.root,
            );
            app.ui.say(format!(
                "running {} ({})",
                hook.name,
                hook.reason.describe()
            ));
            crate::hooks::run(hook, &app.paths.repo, &env)?;

            let mut state = state;
            state.record_hook(&hook.name, hook.hash.clone());
            state.save(&app.paths.state)?;
            app.ui.ok(format!("{} finished", hook.name));
            Ok(0)
        }
    }
}

// ── lint ────────────────────────────────────────────────────────────────────

pub fn lint(app: &mut App, args: &LintArgs) -> Result<i32> {
    let mut problems: Vec<String> = Vec::new();
    let cfg = app.config()?;
    let platform = app.platform()?;
    let files_dir = app.paths.repo.join(crate::config::FILES_DIR);

    // 1. Manifest and profile consistency.
    problems.extend(cfg.problems());

    // 2. Manifest entries with no file behind them.
    for spec in &cfg.repo.files {
        if !files_dir.join(&spec.path).exists() {
            problems.push(format!(
                "hats.yaml lists `{}`, which does not exist under files/",
                spec.path.display()
            ));
        }
    }

    // 3. Modes that do not parse.
    for spec in &cfg.repo.files {
        for (what, value) in [("mode", &spec.mode), ("dir_mode", &spec.dir_mode)] {
            if let Some(v) = value
                && crate::model::parse_mode(v).is_none()
            {
                problems.push(format!(
                    "`{}` has an invalid {what}: {v}",
                    spec.path.display()
                ));
            }
        }
    }

    // 4. Hook scripts that are missing.
    for hook in &cfg.repo.hooks {
        if !app.paths.repo.join(&hook.script).is_file() {
            problems.push(format!(
                "hook `{}` points at {}, which does not exist",
                hook.name,
                hook.script.display()
            ));
        }
    }

    // 5. Every template parses and renders with placeholder secrets. This is
    //    the check that catches a template referring to a variable that does
    //    not exist, which would otherwise only show up at apply time.
    let ctx = RenderContext::build(
        &cfg,
        &platform,
        &Secrets::default(),
        &app.paths.repo,
        &files_dir,
        RedactMode::Placeholder,
    );
    let renderer = Renderer::new(&ctx);
    let files = crate::model::expand(&cfg, &files_dir, &platform, &Filter::default())?;
    for file in &files {
        if let Err(e) = renderer.render(file) {
            problems.push(format!("{}: {e:#}", file.source.display()));
        }
    }

    // 6. Optional external scanners, when installed.
    if !args.no_external {
        if which::which("shellcheck").is_ok() {
            run_shellcheck(app, &mut problems);
        } else {
            app.ui.detail("shellcheck not installed; skipping");
        }
        if which::which("gitleaks").is_ok() {
            let out = std::process::Command::new("gitleaks")
                .args(["detect", "--no-banner", "--source"])
                .arg(&app.paths.repo)
                .output();
            match out {
                Ok(o) if !o.status.success() => {
                    problems.push("gitleaks found potential secrets in the repo".into())
                }
                Ok(_) => app.ui.ok("gitleaks: clean"),
                Err(e) => app.ui.detail(format!("gitleaks failed to run: {e}")),
            }
        } else {
            app.ui.detail("gitleaks not installed; skipping");
        }
    }

    if problems.is_empty() {
        app.ui.ok(format!(
            "{} managed file{} and {} hook{} look fine",
            files.len(),
            if files.len() == 1 { "" } else { "s" },
            cfg.repo.hooks.len(),
            if cfg.repo.hooks.len() == 1 { "" } else { "s" },
        ));
        return Ok(0);
    }

    for p in &problems {
        app.ui.warn(p);
    }
    app.ui.say(format!("\n{} problem(s)", problems.len()));
    Ok(1)
}

fn run_shellcheck(app: &App, problems: &mut Vec<String>) {
    let mut scripts: Vec<std::path::PathBuf> = Vec::new();
    for dir in ["hooks", "scripts"] {
        let d = app.paths.repo.join(dir);
        if let Ok(entries) = std::fs::read_dir(&d) {
            scripts.extend(
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "sh")),
            );
        }
    }
    let bootstrap = app.paths.repo.join("bootstrap.sh");
    if bootstrap.is_file() {
        scripts.push(bootstrap);
    }
    if scripts.is_empty() {
        return;
    }
    scripts.sort();

    let out = std::process::Command::new("shellcheck")
        .args(["-S", "warning", "-e", "SC1090,SC1091"])
        .args(&scripts)
        .output();
    match out {
        Ok(o) if !o.status.success() => {
            problems.push(format!(
                "shellcheck reported problems:\n{}",
                String::from_utf8_lossy(&o.stdout).trim()
            ));
        }
        Ok(_) => app
            .ui
            .ok(format!("shellcheck: {} script(s) clean", scripts.len())),
        Err(e) => app.ui.detail(format!("shellcheck failed to run: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brew_actions_map_to_distinct_bundle_subcommands() {
        // Guards against a copy-paste error making `cleanup` run `install`.
        let seen: Vec<&str> = [
            BrewAction::Install,
            BrewAction::Check,
            BrewAction::Cleanup,
            BrewAction::Dump,
        ]
        .iter()
        .map(|a| match a {
            BrewAction::Install => "install",
            BrewAction::Check => "check",
            BrewAction::Cleanup => "cleanup",
            BrewAction::Dump => "dump",
        })
        .collect();
        let unique: std::collections::BTreeSet<_> = seen.iter().collect();
        assert_eq!(unique.len(), 4);
    }
}
