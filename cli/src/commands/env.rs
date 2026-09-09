//! `hats env <profile>` and `hats shell-init <shell>`.
//!
//! These two are the profile switcher. `env` prints the shell code for one
//! switch; `shell-init` prints the `profile` function that evals it.
//!
//! `env` runs on every new shell, so it has one overriding requirement: when
//! anything goes wrong it must print *nothing* to stdout and exit cleanly under
//! `--quiet`. A login shell evaluating a partial script is a much worse failure
//! than a shell with no profile.

use anyhow::Result;
use minijinja::{Environment, context};

use crate::app::App;
use crate::cli::{EnvArgs, ShellInitArgs};
use crate::profile::emit::{ShellEmitter, Zsh, sh_quote};
use crate::profile::{EnvOptions, EnvPlan, kube};
use crate::secrets::store::Secrets;

/// The integration script, compiled into the binary so a mid-upgrade repo
/// cannot leave a shell without its `profile` function.
const ZSH_INIT: &str = include_str!("../../shell/hats.zsh.j2");

pub fn env(app: &mut App, args: &EnvArgs) -> Result<()> {
    // Under --quiet nothing may reach stdout on failure. Build the whole script
    // first, then print it in one go.
    match build(app, args) {
        Ok(script) => {
            print!("{script}");
            Ok(())
        }
        Err(e) if args.quiet => {
            app.ui.warn(format!("hats env: {e:#}"));
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn build(app: &mut App, args: &EnvArgs) -> Result<String> {
    let cfg = app.config()?;
    let secrets = Secrets::load(&app.paths.secrets)?;
    let platform = app.platform()?;

    let opts = EnvOptions {
        no_kube: args.no_kube,
        no_colour: args.no_colour,
        reset_only: args.reset,
    };
    let name = match &args.profile {
        Some(n) => n.clone(),
        None => cfg.local.default_profile(),
    };

    let mut plan = EnvPlan::build(&cfg, &name, &secrets, &platform.home, opts)?;

    // Seed the per-profile kubeconfig and select its context here, in the
    // process, rather than emitting shell to do it. Both act on files, not on
    // the parent shell, so there is nothing to gain from deferring them, and
    // failures stay out of the evaluated script.
    if let Some(kc) = plan.kubeconfig.clone() {
        match kube::isolate(&platform.home, &name) {
            Ok(path) => {
                plan.set
                    .insert("KUBECONFIG".into(), path.to_string_lossy().into_owned());
                if let Some(ctx) = &plan.kube_context
                    && !kube::use_context(&path, ctx)
                {
                    app.ui
                        .detail(format!("could not select kube context `{ctx}`"));
                }
            }
            Err(e) => app.ui.detail(format!("kube isolation skipped: {e:#}")),
        }
        let _ = kc;
    }

    if !plan.missing_secrets.is_empty() && !args.quiet {
        app.ui.warn(format!(
            "profile `{name}` refers to {} unfetched secret{}: {}. Run `hats secrets fetch`.",
            plan.missing_secrets.len(),
            if plan.missing_secrets.len() == 1 {
                ""
            } else {
                "s"
            },
            plan.missing_secrets.join(", ")
        ));
    }

    Ok(Zsh.emit(&plan))
}

pub fn shell_init(app: &mut App, args: &ShellInitArgs) -> Result<()> {
    // shell-init must work before `hats init` has run, so a missing config is
    // not fatal: emit the functions with no baseline profile.
    let (default_profile, env_bundles, repo_dir) = match app.config() {
        Ok(cfg) => (
            Some(cfg.local.default_profile()),
            cfg.group_enabled("env-bundles"),
            app.paths.repo.to_string_lossy().into_owned(),
        ),
        Err(_) => (None, false, app.paths.repo.to_string_lossy().into_owned()),
    };

    if args.shell != "zsh" {
        anyhow::bail!(
            "hats can only emit zsh integration so far (asked for `{}`)",
            args.shell
        );
    }

    let mut jinja = Environment::new();
    jinja.add_filter("sh_quote", |v: String| sh_quote(&v));
    jinja.add_template("init", ZSH_INIT)?;
    let rendered = jinja.get_template("init")?.render(context! {
        default_profile => default_profile,
        env_bundles => env_bundles,
        repo_dir => repo_dir,
    })?;

    print!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render the integration script the way `shell-init` does, without needing
    /// a configured machine.
    fn render(default_profile: Option<&str>, env_bundles: bool) -> String {
        let mut jinja = Environment::new();
        jinja.add_filter("sh_quote", |v: String| sh_quote(&v));
        jinja.add_template("init", ZSH_INIT).unwrap();
        jinja
            .get_template("init")
            .unwrap()
            .render(context! {
                default_profile => default_profile,
                env_bundles => env_bundles,
                repo_dir => "/home/t/.hats/repo",
            })
            .unwrap()
    }

    fn parses_as_zsh(script: &str) -> bool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("init.zsh");
        std::fs::write(&path, script).unwrap();
        std::process::Command::new("zsh")
            .arg("-n")
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    }

    #[test]
    fn the_integration_script_is_valid_zsh_in_every_variant() {
        for (profile, bundles) in [
            (Some("normal"), true),
            (Some("normal"), false),
            (None, false),
            (None, true),
        ] {
            let script = render(profile, bundles);
            assert!(
                parses_as_zsh(&script),
                "invalid zsh for ({profile:?}, {bundles}):\n{script}"
            );
        }
    }

    #[test]
    fn it_defines_the_profile_function_and_its_completion() {
        let script = render(Some("normal"), false);
        assert!(script.contains("profile() {"));
        assert!(script.contains("compdef _hats_profiles profile"));
        assert!(script.contains("fzf --prompt='profile > '"));
    }

    #[test]
    fn the_baseline_eval_is_quiet_and_cannot_break_a_shell() {
        let script = render(Some("normal"), false);
        assert!(script.contains("hats env normal --quiet"));
        assert!(script.contains("2>/dev/null"));
        assert!(script.contains("|| true"), "shell startup must never fail");
    }

    #[test]
    fn without_a_config_there_is_no_baseline_eval() {
        let script = render(None, false);
        // The `profile` function itself calls `hats env`, so assert on the
        // baseline block specifically: the guard that runs it at shell start.
        assert!(
            !script.contains("if [ -z \"${HATS_PROFILE:-}\" ]"),
            "an unconfigured machine must not eval a profile at shell start:\n{script}"
        );
        assert!(!script.contains("--quiet"), "{script}");
        // But the function is still defined, so `profile` works after init.
        assert!(script.contains("profile() {"));
    }

    #[test]
    fn envp_appears_only_when_the_bundles_group_is_enabled() {
        assert!(render(Some("normal"), true).contains("envp()"));
        assert!(!render(Some("normal"), false).contains("envp()"));
    }

    #[test]
    fn the_switch_captures_before_it_evals() {
        let script = render(Some("normal"), false);
        let capture = script.find("script=$(command hats env").unwrap();
        let evaluate = script.find("eval \"$script\"").unwrap();
        assert!(
            capture < evaluate,
            "must not eval a stream that may fail midway"
        );
    }
}
