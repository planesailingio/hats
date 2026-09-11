//! `hats plan`, `hats apply`, `hats diff` and `hats render`.
//!
//! One code path builds the plan; these four differ only in what they do with
//! it. That is deliberate: the thing you are shown by `plan` is the same object
//! `apply` acts on, so they cannot drift.

use anyhow::{Context, Result};
use owo_colors::OwoColorize;

use crate::app::App;
use crate::cli::{ApplyArgs, DiffArgs, PlanArgs, RenderArgs};
use crate::commands::update;
use crate::engine::apply::{ApplyContext, ApplyOptions};
use crate::engine::diff::{self, Body};
use crate::engine::plan::{Action, Plan, PlanOptions};
use crate::engine::render::{RedactMode, RenderContext, Renderer};
use crate::engine::state::State;
use crate::model::Filter;
use crate::secrets::store::Secrets;

/// Exit code when a plan finds work to do, in the style of
/// `terraform plan -detailed-exitcode`.
pub const EXIT_CHANGES: i32 = 2;

pub fn plan(app: &mut App, args: &PlanArgs) -> Result<i32> {
    if !update::ensure_in_step(app)? {
        return Ok(1);
    }
    let plan = build(app, &args.into())?;
    show(app, &plan, args.show_secrets);
    Ok(if plan.summary.has_changes() {
        EXIT_CHANGES
    } else {
        0
    })
}

pub fn diff(app: &mut App, args: &DiffArgs) -> Result<i32> {
    let plan_args = PlanArgs {
        target: args.target.clone(),
        group: args.group.clone(),
        skip_hooks: true,
        show_secrets: args.show_secrets,
    };
    plan(app, &plan_args)
}

pub fn apply(app: &mut App, args: &ApplyArgs) -> Result<i32> {
    if !update::ensure_in_step(app)? {
        return Ok(1);
    }

    // Re-plan rather than trusting anything cached: the machine may have moved
    // since the plan was printed.
    let plan = build(app, &args.into())?;

    if !plan.summary.has_changes() {
        app.ui.say("Nothing to do: everything is already in place.");
        return Ok(0);
    }

    show(app, &plan, args.show_secrets);

    if !args.yes {
        let go = app
            .ui
            .prompter
            .confirm("apply.confirm", "Apply these changes?", false)?;
        if !go {
            app.ui.say("Nothing applied.");
            return Ok(1);
        }
    }

    let platform = app.platform()?;
    let ctx = ApplyContext {
        platform: &platform,
        repo_dir: app.paths.repo.clone(),
        files_dir: app.paths.repo.join(crate::config::FILES_DIR),
        hats_home: app.paths.root.clone(),
        backups_root: app.paths.backups.clone(),
        state_path: app.paths.state.clone(),
    };
    let mut state = State::load(&app.paths.state)?;
    let opts = ApplyOptions {
        only_files: args.only_files,
        hooks_only: args.hooks_only,
        no_prune: args.no_prune,
        force_prune: args.force_prune,
    };

    let verbose = app.ui.colour();
    let outcome = crate::engine::apply::apply(&plan, &mut state, &ctx, &opts, |msg| {
        if verbose {
            println!("  {msg}");
        }
    })?;

    app.ui.say("");
    app.ui.ok(outcome.line());
    for skipped in &outcome.skipped {
        app.ui.warn(format!("skipped {skipped}"));
    }
    if let Some(backup) = &outcome.backup_dir {
        app.ui.say(format!(
            "Replaced files were copied to {}",
            backup.display()
        ));
    }
    Ok(0)
}

pub fn render(app: &mut App, args: &RenderArgs) -> Result<i32> {
    let cfg = app.config()?;
    let platform = app.platform()?;
    let secrets = if args.placeholder_secrets {
        Secrets::default()
    } else {
        Secrets::load(&app.paths.secrets)?
    };
    let files_dir = app.paths.repo.join(crate::config::FILES_DIR);

    let mode = if args.placeholder_secrets {
        RedactMode::Placeholder
    } else {
        RedactMode::Real
    };
    let ctx = RenderContext::build(&cfg, &platform, &secrets, &app.paths.repo, &files_dir, mode);
    let renderer = Renderer::new(&ctx);

    let filter = Filter {
        targets: args.target.iter().map(Into::into).collect(),
        groups: Vec::new(),
    };
    let files = crate::model::expand(&cfg, &files_dir, &platform, &filter)?;
    if files.is_empty() {
        anyhow::bail!("no managed file matches");
    }

    let mut failures = 0;
    for file in &files {
        let rendered = renderer
            .render(file)
            .with_context(|| format!("rendering {}", file.source.display()))?;

        if args.check {
            match syntax_check(file, &rendered) {
                Ok(Some(shell)) => app
                    .ui
                    .ok(format!("{} ({shell} -n)", file.display(&platform.home))),
                Ok(None) => app
                    .ui
                    .detail(format!("{}: no checker", file.display(&platform.home))),
                Err(e) => {
                    failures += 1;
                    app.ui
                        .warn(format!("{}: {e}", file.display(&platform.home)));
                }
            }
            continue;
        }

        if files.len() > 1 {
            app.ui
                .say(format!("──── {} ────", file.display(&platform.home)));
        }
        print!("{}", String::from_utf8_lossy(&rendered));
    }

    Ok(if failures > 0 { 1 } else { 0 })
}

/// Syntax-check a rendered file with the right shell, if there is one.
fn syntax_check(file: &crate::model::ManagedFile, rendered: &[u8]) -> Result<Option<&'static str>> {
    let name = file.rel.to_string_lossy();
    let shell = if name.ends_with(".zsh") || name.contains("zshrc") || name.contains("zshenv") {
        "zsh"
    } else if name.ends_with(".sh") || name.ends_with(".bash") {
        "sh"
    } else {
        return Ok(None);
    };

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("rendered");
    std::fs::write(&path, rendered)?;
    let out = std::process::Command::new(shell)
        .arg("-n")
        .arg(&path)
        .output()
        .with_context(|| format!("running {shell} -n"))?;
    if out.status.success() {
        Ok(Some(shell))
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

/// Shared plan construction for plan, diff and apply.
struct BuildArgs {
    targets: Vec<String>,
    groups: Vec<String>,
    skip_hooks: bool,
    show_secrets: bool,
}

impl From<&PlanArgs> for BuildArgs {
    fn from(a: &PlanArgs) -> Self {
        Self {
            targets: a.target.clone(),
            groups: a.group.clone(),
            skip_hooks: a.skip_hooks,
            show_secrets: a.show_secrets,
        }
    }
}

impl From<&ApplyArgs> for BuildArgs {
    fn from(a: &ApplyArgs) -> Self {
        Self {
            targets: a.target.clone(),
            groups: a.group.clone(),
            skip_hooks: a.only_files,
            show_secrets: a.show_secrets,
        }
    }
}

fn build(app: &App, args: &BuildArgs) -> Result<Plan> {
    let cfg = app.config()?;
    let platform = app.platform()?;
    let secrets = Secrets::load(&app.paths.secrets)?;
    let state = State::load(&app.paths.state)?;

    let opts = PlanOptions {
        cfg: &cfg,
        platform: &platform,
        secrets: &secrets,
        state: &state,
        repo_dir: app.paths.repo.clone(),
        files_dir: app.paths.repo.join(crate::config::FILES_DIR),
        hats_home: app.paths.root.clone(),
        filter: Filter {
            targets: args.targets.iter().map(Into::into).collect(),
            groups: args.groups.clone(),
        },
        skip_hooks: args.skip_hooks,
        show_secrets: args.show_secrets,
        force_hook: None,
    };
    crate::engine::plan::plan(&opts)
}

/// Print the plan, terraform style.
fn show(app: &App, plan: &Plan, show_secrets: bool) {
    for note in &plan.notes {
        app.ui.warn(note);
    }

    if plan.entries.is_empty() && plan.scaffolds.is_empty() && plan.hooks.is_empty() {
        app.ui
            .say("No managed files. Check the groups in ~/.hats/config.yaml.");
        return;
    }

    let colour = app.ui.colour();
    for entry in &plan.entries {
        if matches!(entry.action, Action::Unchanged) && !colour {
            continue;
        }
        let marker = entry.action.marker();
        let head = format!(
            "  {marker} {:<34} {}{}",
            entry.display,
            entry.action.label(),
            stats(entry),
        );
        app.ui.say(match (&entry.action, colour) {
            (Action::Create, true) => head.green().to_string(),
            (Action::Update, true) => head.yellow().to_string(),
            (Action::Destroy { .. }, true) => head.red().to_string(),
            (Action::Conflict { .. }, true) => head.magenta().to_string(),
            (Action::Unchanged, true) => head.dimmed().to_string(),
            _ => head,
        });

        match &entry.body {
            Body::Unified(d) => {
                let text = if colour {
                    diff::colourise(d)
                } else {
                    d.clone()
                };
                app.ui.say(diff::indent(&text, "      "));
            }
            Body::Withheld { added, removed } => app.ui.say(format!(
                "      (contents withheld: +{added} −{removed}; --show-secrets to reveal)"
            )),
            Body::Binary { old, new } => app.ui.say(format!("      (binary: {old} → {new} bytes)")),
            Body::Whole { lines } => app.ui.say(format!("      ({lines} lines)")),
            Body::None => {}
        }
    }

    if !plan.scaffolds.is_empty() {
        app.ui.say("");
        app.ui.say("Scaffold (created once, then left alone)");
        for s in &plan.scaffolds {
            let line = format!("  + {:<34} {}", s.display, s.how);
            app.ui.say(if colour {
                line.green().to_string()
            } else {
                line
            });
        }
    }

    if !plan.hooks.is_empty() {
        app.ui.say("");
        app.ui.say("Hooks");
        for hook in &plan.hooks {
            app.ui.say(format!(
                "  ▸ {:<20} {:<10} {}",
                hook.name,
                hook.reason.label(),
                hook.reason.describe()
            ));
        }
    }

    app.ui.say("");
    app.ui.say(plan.summary.line());
    if show_secrets {
        app.ui
            .warn("real secret values were printed above; clear your scrollback");
    }
}

fn stats(entry: &crate::engine::plan::Entry) -> String {
    if entry.stats.is_empty() {
        String::new()
    } else {
        format!("  (+{} −{})", entry.stats.added, entry.stats.removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::plan::{Entry, Summary};

    fn entry(added: usize, removed: usize) -> Entry {
        Entry {
            target: "/home/t/.zshrc".into(),
            display: "~/.zshrc".into(),
            action: Action::Update,
            stats: crate::engine::diff::Stats { added, removed },
            body: Body::None,
            contents: None,
            mode: 0o644,
            dir_mode: None,
            secret: false,
        }
    }

    #[test]
    fn line_stats_are_omitted_when_there_are_none() {
        assert_eq!(stats(&entry(0, 0)), "");
        assert_eq!(stats(&entry(4, 12)), "  (+4 −12)");
    }

    #[test]
    fn a_plan_with_changes_exits_two_like_terraform() {
        let s = Summary {
            add: 1,
            ..Default::default()
        };
        assert!(s.has_changes());
        assert_eq!(EXIT_CHANGES, 2);
    }

    #[test]
    fn an_empty_plan_has_no_changes() {
        assert!(
            !Summary {
                unchanged: 5,
                ..Default::default()
            }
            .has_changes()
        );
    }
}
