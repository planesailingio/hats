//! `hats hat` — inspect the hats configured on this machine.
//!
//! Switching a shell's environment is `hats env` plus the shell integration
//! (a child process cannot mutate its parent's environment). This command is
//! the read-only half: list, show, current, and the derived reset list.

use anyhow::{Context, Result};

use crate::app::App;
use crate::cli::{HatArgs, HatCommand};
use crate::config::Config;

pub fn run(app: &mut App, args: &HatArgs) -> Result<()> {
    let cfg = app.config()?;
    match args.command.as_ref() {
        None | Some(HatCommand::List { plain: false }) => list(app, &cfg, false),
        Some(HatCommand::List { plain: true }) => list(app, &cfg, true),
        Some(HatCommand::Show { name, json }) => show(app, &cfg, name, *json),
        Some(HatCommand::Current { summary }) => current(app, &cfg, *summary),
        Some(HatCommand::ResetList) => reset_list(app, &cfg),
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
        app.ui
            .say("no hat on in this shell. Run `hat <name>`.");
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
