//! `hats hat` — the hats configured on this machine.
//!
//! Switching a shell's environment is `hats env` plus the shell integration
//! (a child process cannot mutate its parent's environment). This command is
//! everything else: list, show, current and the derived reset list read the
//! config; create and delete change it, along with the per-hat files.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::app::App;
use crate::cli::{HatArgs, HatCommand, HatCreateArgs};
use crate::commands::init::{PALETTE, non_empty};
use crate::config::Config;
use crate::config::hat::{HatSpec, IdentitySpec, KubeSpec};
use crate::error::ConfigError;
use crate::hat::scaffold::{self, tilde};
use crate::model::Filter;

/// Run the subcommand. Returns the process exit code.
pub fn run(app: &mut App, args: &HatArgs) -> Result<i32> {
    let cfg = app.config()?;
    match args.command.as_ref() {
        None | Some(HatCommand::List { plain: false }) => list(app, &cfg, false).map(|()| 0),
        Some(HatCommand::List { plain: true }) => list(app, &cfg, true).map(|()| 0),
        Some(HatCommand::Show { name, json }) => show(app, &cfg, name, *json).map(|()| 0),
        Some(HatCommand::Current { summary }) => current(app, &cfg, *summary).map(|()| 0),
        Some(HatCommand::ResetList) => reset_list(app, &cfg).map(|()| 0),
        Some(HatCommand::Create(args)) => create(app, cfg, args).map(|()| 0),
        Some(HatCommand::Delete { name, yes }) => delete(app, cfg, name, *yes),
    }
}

fn list(app: &App, cfg: &Config, plain: bool) -> Result<()> {
    let active = active_hat();
    let default = cfg.local.default_hat();

    if plain {
        // Consumed by fzf and by shell completion: names only, nothing else.
        for name in cfg.hat_names() {
            app.ui.say(name);
        }
        return Ok(());
    }

    if cfg.hats().is_empty() {
        app.ui.warn("no hats configured. Run `hats init`.");
        return Ok(());
    }

    for name in cfg.hat_names() {
        let resolved = cfg.resolve_hat(&name);
        let marker = match (&active, name == default) {
            (Some(a), _) if *a == name => "*",
            (_, true) => "·",
            _ => " ",
        };
        let detail = match &resolved {
            Ok(p) => {
                let mut bits = Vec::new();
                if let Some(e) = &p.identity.email {
                    bits.push(e.clone());
                }
                if let Some(k) = &p.kube.context {
                    bits.push(format!("kube={k}"));
                }
                bits.join("  ")
            }
            Err(e) => format!("BROKEN: {e}"),
        };
        app.ui.say(format!("{marker} {name:<14} {detail}"));
    }

    app.ui.say("");
    app.ui.say(format!("* active   · default ({default})"));
    Ok(())
}

fn show(app: &App, cfg: &Config, name: &str, json: bool) -> Result<()> {
    let p = cfg
        .resolve_hat(name)
        .with_context(|| format!("resolving hat `{name}`"))?;

    if json {
        app.ui.say(serde_json::to_string_pretty(&p)?);
        return Ok(());
    }

    app.ui.say(format!("hat       {}", p.name));
    if let Some(d) = &p.description {
        app.ui.say(format!("about     {d}"));
    }
    if let Some(spec) = cfg.hats().get(name)
        && let Some(parent) = &spec.inherits
    {
        app.ui.say(format!("inherits  {parent}"));
    }
    if let Some(n) = &p.identity.name {
        app.ui.say(format!("git name  {n}"));
    }
    if let Some(e) = &p.identity.email {
        app.ui.say(format!("git email {e}"));
    }
    if p.identity.signing_key.is_some() {
        app.ui.say("signing   set (value withheld)");
    }
    app.ui
        .say(format!("aws       isolated: {}", p.aws_isolated()));
    app.ui.say(format!(
        "kube      {} (isolated: {})",
        p.kube.context.as_deref().unwrap_or("-"),
        p.kube_isolated()
    ));
    app.ui
        .say(format!("k9s       isolated: {}", p.k9s_isolated()));
    app.ui.say(format!(
        "coder     {} (isolated: {})",
        p.coder.url.as_deref().unwrap_or("-"),
        p.coder_isolated()
    ));
    if let Some(c) = &p.colour {
        app.ui.say(format!("tint      {c}"));
    }
    if !p.path.is_empty() {
        app.ui.say(format!("path      {}", p.path.join(", ")));
    }
    if !p.env.is_empty() {
        app.ui.say("env");
        for (k, v) in &p.env {
            let shown = match v.secret_ref() {
                Some(s) => format!("«secret:{s}»"),
                None => match v {
                    crate::config::hat::EnvValue::Literal(l) => l.clone(),
                    _ => unreachable!(),
                },
            };
            app.ui.say(format!("  {k}={shown}"));
        }
    }
    Ok(())
}

fn current(app: &App, cfg: &Config, summary: bool) -> Result<()> {
    let Some(name) = active_hat() else {
        app.ui.say("no hat on in this shell. Run `hat <name>`.");
        return Ok(());
    };

    if !summary {
        app.ui.say(name);
        return Ok(());
    }

    // The one-line confirmation the shell function prints after a switch.
    let git = std::env::var("GIT_AUTHOR_EMAIL").unwrap_or_else(|_| "-".into());
    let kube = cfg
        .resolve_hat(&name)
        .ok()
        .and_then(|p| p.kube.context)
        .unwrap_or_else(|| "-".into());
    app.ui
        .say(format!("⛭ hat: {name}  (git={git}  kube={kube})"));
    Ok(())
}

fn reset_list(app: &App, cfg: &Config) -> Result<()> {
    // The union across every hat: this is what makes the old
    // `_profile_reset` leak impossible.
    for key in cfg.all_env_keys() {
        app.ui.say(key);
    }
    Ok(())
}

/// Add a hat to the config, then give it the per-hat files an apply would.
fn create(app: &mut App, mut cfg: Config, args: &HatCreateArgs) -> Result<()> {
    let name = args.name.trim();
    scaffold::check_name(name)?;
    if cfg.hats().contains_key(name) {
        bail!(
            "hat `{name}` already exists. `hats hat show {name}` shows it; \
             edit it in {}.",
            app.paths.config.display()
        );
    }

    let spec = ask_new_hat(app, &cfg, name, args)?;
    cfg.local.hats.insert(name.to_string(), spec);
    // Before anything is written, so a bad parent never reaches the file.
    cfg.resolve_hat(name)?;

    let platform = app.platform()?;
    let home = &platform.home;
    let backup = backup_dir(app);
    save_config(app, &cfg, &backup)?;
    app.ui.ok(format!(
        "added hat `{name}` to {}",
        tilde(&app.paths.config, home)
    ));

    let files_dir = app.paths.repo.join(crate::config::FILES_DIR);
    let managed = crate::model::expand(&cfg, &files_dir, &platform, &Filter::default())?;
    for s in scaffold::for_hat(&cfg, home, &managed, name) {
        if scaffold::create(&s, home)? {
            app.ui.ok(format!("created {}  ({})", s.display, s.how));
        }
    }

    app.ui.say("");
    app.ui.say(format!("Put it on with `hat {name}`."));
    Ok(())
}

/// The new hat's spec, from flags where given and questions where not.
///
/// Only what differs from what the hat would get anyway is recorded, as the
/// wizard does, so a child hat stays a few lines.
fn ask_new_hat(app: &mut App, cfg: &Config, name: &str, args: &HatCreateArgs) -> Result<HatSpec> {
    // A new client is usually "me, with a different email and cluster", so
    // inheriting from the default hat is the suggestion.
    let inherits = match (&args.inherits, args.no_inherit) {
        (_, true) => None,
        (Some(parent), false) => Some(parent.clone()),
        (None, false) => {
            let base = cfg.local.default_hat();
            let yes = cfg.hats().contains_key(&base)
                && app.ui.prompter.confirm(
                    "hat.create.inherits",
                    &format!("Inherit defaults from `{base}`?"),
                    true,
                )?;
            yes.then_some(base)
        }
    };

    // What the hat gets without saying anything: the parent's identity, or
    // the machine identity for a hat that inherits nothing.
    let base = match &inherits {
        Some(parent) => {
            cfg.resolve_hat(parent)
                .with_context(|| format!("`{name}` would inherit from `{parent}`"))?
                .identity
        }
        None => cfg.local.identity.clone().unwrap_or_default(),
    };

    let git_name = answer(
        app,
        args.git_name.as_deref(),
        "hat.create.git_name",
        &format!("[{name}] git name"),
        base.name.as_deref().unwrap_or(""),
    )?;
    let git_email = answer(
        app,
        args.git_email.as_deref(),
        "hat.create.git_email",
        &format!("[{name}] git email"),
        base.email.as_deref().unwrap_or(""),
    )?;
    let kube_context = answer(
        app,
        args.kube_context.as_deref(),
        "hat.create.kube_context",
        &format!("[{name}] kube context (blank for none)"),
        "",
    )?;
    let colour = answer(
        app,
        args.colour.as_deref(),
        "hat.create.colour",
        &format!("[{name}] terminal tint"),
        PALETTE[cfg.hats().len() % PALETTE.len()],
    )?;

    let identity = IdentitySpec {
        name: non_empty(git_name).filter(|n| Some(n) != base.name.as_ref()),
        email: non_empty(git_email).filter(|e| Some(e) != base.email.as_ref()),
        signing_key: None,
    };
    Ok(HatSpec {
        inherits,
        description: args.description.clone().and_then(non_empty),
        colour: non_empty(colour),
        identity: (identity != IdentitySpec::default()).then_some(identity),
        kube: non_empty(kube_context).map(|context| KubeSpec {
            context: Some(context),
            isolate: None,
        }),
        ..Default::default()
    })
}

/// A flag's value when it was given, else the question.
fn answer(
    app: &mut App,
    flag: Option<&str>,
    key: &str,
    message: &str,
    default: &str,
) -> Result<String> {
    match flag {
        Some(value) => Ok(value.to_string()),
        None => app.ui.prompter.text(key, message, Some(default)),
    }
}

/// Remove a hat from the config and move every file it owns to the backups,
/// including the ones hats scaffolded and never managed. Returns 1 when the
/// user says no.
fn delete(app: &mut App, mut cfg: Config, name: &str, yes: bool) -> Result<i32> {
    if !cfg.hats().contains_key(name) {
        return Err(ConfigError::UnknownHat {
            name: name.to_string(),
            known: cfg.hat_names(),
        }
        .into());
    }
    scaffold::check_name(name).with_context(|| {
        format!(
            "not touching files for it; remove it from {} by hand",
            app.paths.config.display()
        )
    })?;

    // Refuse anything that would leave the config broken.
    let children: Vec<&str> = cfg
        .hats()
        .iter()
        .filter(|(_, spec)| spec.inherits.as_deref() == Some(name))
        .map(|(child, _)| child.as_str())
        .collect();
    if !children.is_empty() {
        bail!(
            "{} inherit{} from `{name}`. Delete {}, or point `inherits:` elsewhere, first.",
            children.join(", "),
            if children.len() == 1 { "s" } else { "" },
            if children.len() == 1 { "it" } else { "them" },
        );
    }
    if cfg.local.default_hat() == name {
        bail!(
            "`{name}` is the default hat, the one new shells start in. \
             Set `meta.default_hat` to another hat in {} first.",
            app.paths.config.display()
        );
    }

    let platform = app.platform()?;
    let home = &platform.home;
    let files: Vec<PathBuf> = scaffold::owned_paths(home, name)
        .into_iter()
        .filter(|p| p.symlink_metadata().is_ok())
        .collect();

    app.ui.say(format!(
        "Deleting hat `{name}` removes it from {} and moves its files to {}:",
        tilde(&app.paths.config, home),
        tilde(&app.paths.backups, home)
    ));
    if files.is_empty() {
        app.ui.say("  (it has no files)");
    }
    for f in &files {
        app.ui.say(format!("  - {}", tilde(f, home)));
    }

    if !yes {
        let go = app.ui.prompter.confirm(
            "hat.delete.confirm",
            &format!("Delete hat `{name}`?"),
            false,
        )?;
        if !go {
            app.ui.say("Nothing deleted.");
            return Ok(1);
        }
    }

    let backup = backup_dir(app);
    // Files before the config: if one cannot be moved, the hat is still
    // there and running the delete again picks up where this one stopped.
    for f in &files {
        scaffold::retire(f, &backup, home)?;
        app.ui.ok(format!("moved {}", tilde(f, home)));
    }
    cfg.local.hats.shift_remove(name);
    save_config(app, &cfg, &backup)?;
    app.ui.ok(format!(
        "removed hat `{name}` from {}",
        tilde(&app.paths.config, home)
    ));
    app.ui
        .say(format!("Undo by copying back from {}", backup.display()));

    if active_hat().as_deref() == Some(name) {
        app.ui.warn(format!(
            "this shell is still wearing `{name}`. Switch with `hat {}`.",
            cfg.local.default_hat()
        ));
    }
    Ok(0)
}

/// One backup directory per command, named as `hats apply` names its own.
fn backup_dir(app: &App) -> PathBuf {
    app.paths
        .backups
        .join(chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string())
}

/// Write the config, keeping the previous one in `backup`. Saving writes the
/// file afresh from what was parsed, so comments added by hand do not
/// survive it; the copy does.
fn save_config(app: &App, cfg: &Config, backup: &Path) -> Result<()> {
    let path = &app.paths.config;
    std::fs::create_dir_all(backup).with_context(|| format!("creating {}", backup.display()))?;
    std::fs::copy(path, backup.join("config.yaml"))
        .with_context(|| format!("backing up {}", path.display()))?;
    cfg.local
        .save(path)
        .with_context(|| format!("writing {}", path.display()))
}

/// The hat this shell is wearing, from `HATS_HAT`.
fn active_hat() -> Option<String> {
    std::env::var("HATS_HAT").ok().filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, not several: the process environment is global, so separate
    /// tests mutating it would race under the default parallel runner.
    #[test]
    fn active_hat_reads_hats_hat_and_treats_empty_as_unset() {
        // SAFETY: the whole environment dance is confined to this one test, so
        // no other test observes these variables.
        unsafe {
            std::env::remove_var("HATS_HAT");
            assert_eq!(active_hat(), None, "nothing set means no hat on");

            std::env::set_var("HATS_HAT", "work");
            assert_eq!(active_hat().as_deref(), Some("work"));

            std::env::set_var("HATS_HAT", "");
            assert_eq!(active_hat(), None, "an empty value is not a hat");

            std::env::remove_var("HATS_HAT");
        }
    }
}
