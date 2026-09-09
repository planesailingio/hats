//! `hats profile` — inspect the profiles configured on this machine.
//!
//! Switching a shell's environment is `hats env` plus the shell integration
//! (a child process cannot mutate its parent's environment). This command is
//! the read-only half: list, show, current, and the derived reset list.

use anyhow::{Context, Result};

use crate::app::App;
use crate::cli::{ProfileArgs, ProfileCommand};
use crate::config::Config;

pub fn run(app: &mut App, args: &ProfileArgs) -> Result<()> {
    let cfg = app.config()?;
    match args.command.as_ref() {
        None | Some(ProfileCommand::List { plain: false }) => list(app, &cfg, false),
        Some(ProfileCommand::List { plain: true }) => list(app, &cfg, true),
        Some(ProfileCommand::Show { name, json }) => show(app, &cfg, name, *json),
        Some(ProfileCommand::Current { summary }) => current(app, &cfg, *summary),
        Some(ProfileCommand::ResetList) => reset_list(app, &cfg),
    }
}

fn list(app: &App, cfg: &Config, plain: bool) -> Result<()> {
    let active = active_profile();
    let default = cfg.local.default_profile();

    if plain {
        // Consumed by fzf and by shell completion: names only, nothing else.
        for name in cfg.profile_names() {
            app.ui.say(name);
        }
        return Ok(());
    }

    if cfg.profiles().is_empty() {
        app.ui.warn("no profiles configured. Run `hats init`.");
        return Ok(());
    }

    for name in cfg.profile_names() {
        let resolved = cfg.resolve_profile(&name);
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
                if let Some(a) = &p.aws.profile {
                    bits.push(format!("aws={a}"));
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
        .resolve_profile(name)
        .with_context(|| format!("resolving profile `{name}`"))?;

    if json {
        app.ui.say(serde_json::to_string_pretty(&p)?);
        return Ok(());
    }

    app.ui.say(format!("profile   {}", p.name));
    if let Some(d) = &p.description {
        app.ui.say(format!("about     {d}"));
    }
    if let Some(spec) = cfg.profiles().get(name)
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
    if let Some(a) = &p.aws.profile {
        app.ui.say(format!("aws       {a}"));
    }
    if let Some(r) = &p.aws.region {
        app.ui.say(format!("region    {r}"));
    }
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
                    crate::config::profile::EnvValue::Literal(l) => l.clone(),
                    _ => unreachable!(),
                },
            };
            app.ui.say(format!("  {k}={shown}"));
        }
    }
    Ok(())
}

fn current(app: &App, cfg: &Config, summary: bool) -> Result<()> {
    let Some(name) = active_profile() else {
        app.ui
            .say("no profile active in this shell. Run `profile <name>`.");
        return Ok(());
    };

    if !summary {
        app.ui.say(name);
        return Ok(());
    }

    // The one-line confirmation the shell function prints after a switch.
    let git = std::env::var("GIT_AUTHOR_EMAIL").unwrap_or_else(|_| "-".into());
    let aws = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "-".into());
    let kube = cfg
        .resolve_profile(&name)
        .ok()
        .and_then(|p| p.kube.context)
        .unwrap_or_else(|| "-".into());
    app.ui.say(format!(
        "⛭ profile: {name}  (git={git}  aws={aws}  kube={kube})"
    ));
    Ok(())
}

fn reset_list(app: &App, cfg: &Config) -> Result<()> {
    // The union across every profile: this is what makes the old
    // `_profile_reset` leak impossible.
    for key in cfg.all_env_keys() {
        app.ui.say(key);
    }
    Ok(())
}

/// The profile this shell is in. `HATS_PROFILE` is authoritative; `DEV_PROFILE`
/// is honoured so a shell started before the migration still reports correctly.
fn active_profile() -> Option<String> {
    std::env::var("HATS_PROFILE")
        .or_else(|_| std::env::var("DEV_PROFILE"))
        .ok()
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, not several: the process environment is global, so separate
    /// tests mutating it would race under the default parallel runner.
    #[test]
    fn active_profile_reads_hats_first_then_the_legacy_alias() {
        // SAFETY: the whole environment dance is confined to this one test, so
        // no other test observes these variables.
        unsafe {
            std::env::remove_var("HATS_PROFILE");
            std::env::remove_var("DEV_PROFILE");
            assert_eq!(
                active_profile(),
                None,
                "nothing set means no active profile"
            );

            std::env::set_var("HATS_PROFILE", "work");
            std::env::set_var("DEV_PROFILE", "old");
            assert_eq!(
                active_profile().as_deref(),
                Some("work"),
                "HATS_PROFILE wins over the compatibility alias"
            );

            std::env::remove_var("HATS_PROFILE");
            assert_eq!(
                active_profile().as_deref(),
                Some("old"),
                "a pre-migration shell still reports its profile"
            );

            std::env::set_var("DEV_PROFILE", "");
            assert_eq!(active_profile(), None, "an empty value is not a profile");

            std::env::remove_var("DEV_PROFILE");
        }
    }
}
