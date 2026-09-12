//! `hats env <hat>` and `hats shell-init <shell>`.
//!
//! These two are the hat switcher. `env` prints the shell code for one
//! switch; `shell-init` prints the `hat` function that evals it.
//!
//! `env` runs on every new shell, so it has one overriding requirement: when
//! anything goes wrong it must print *nothing* to stdout and exit cleanly under
//! `--quiet`. A login shell evaluating a partial script is a much worse failure
//! than a shell with no hat.

use anyhow::Result;
use minijinja::{Environment, context};

use crate::app::App;
use crate::cli::{EnvArgs, ShellInitArgs};
use crate::hat::emit::{ShellEmitter, Zsh, sh_quote};
use crate::hat::{EnvOptions, EnvPlan, aws, coder, k9s, kube};
use crate::secrets::store::Secrets;

/// The integration script, compiled into the binary so a mid-upgrade repo
/// cannot leave a shell without its `hat` function.
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
        no_aws: args.no_aws,
        no_colour: args.no_colour,
        reset_only: args.reset,
    };
    let name = match &args.hat {
        Some(n) => n.clone(),
        None => cfg.local.default_hat(),
    };

    let mut plan = EnvPlan::build(&cfg, &name, &secrets, &platform.home, opts)?;

    // Seed the per-hat kubeconfig and select its context here, in the
    // process, rather than emitting shell to do it. Both act on files, not on
    // the parent shell, so there is nothing to gain from deferring them, and
    // failures stay out of the evaluated script.
    if plan.aws_config.is_some() {
        match aws::isolate(&platform.home, &name) {
            Ok((config, credentials)) => {
                plan.set.insert(
                    "AWS_CONFIG_FILE".into(),
                    config.to_string_lossy().into_owned(),
                );
                plan.set.insert(
                    "AWS_SHARED_CREDENTIALS_FILE".into(),
                    credentials.to_string_lossy().into_owned(),
                );
            }
            // Falling back to the shared files would silently reintroduce the
            // leak, so drop the pointers instead and say so.
            Err(e) => {
                app.ui.detail(format!("aws isolation skipped: {e:#}"));
                plan.set.shift_remove("AWS_CONFIG_FILE");
                plan.set.shift_remove("AWS_SHARED_CREDENTIALS_FILE");
            }
        }
    }

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

    // The theme links must be in a hat's k9s directory before k9s first runs
    // there, or k9s writes a default config.yaml and the hat loses the theme.
    if plan.k9s_config_dir.is_some() {
        match k9s::isolate(&platform.home, &name) {
            Ok(dir) => {
                plan.set
                    .insert("K9S_CONFIG_DIR".into(), dir.to_string_lossy().into_owned());
            }
            Err(e) => {
                app.ui.detail(format!("k9s isolation skipped: {e:#}"));
                plan.set.shift_remove("K9S_CONFIG_DIR");
            }
        }
    }

    // Once CODER_CONFIG_DIR is set the session token is written there, so the
    // directory must exist, owner-only, before the first `coder login`.
    if plan.coder_config_dir.is_some() {
        match coder::isolate(&platform.home, &name) {
            Ok(dir) => {
                plan.set.insert(
                    "CODER_CONFIG_DIR".into(),
                    dir.to_string_lossy().into_owned(),
                );
            }
            Err(e) => {
                app.ui.detail(format!("coder isolation skipped: {e:#}"));
                plan.set.shift_remove("CODER_CONFIG_DIR");
            }
        }
    }

    if !plan.missing_secrets.is_empty() && !args.quiet {
        app.ui.warn(format!(
            "hat `{name}` refers to {} unfetched secret{}: {}. Run `hats secrets fetch`.",
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
    // not fatal: emit the functions with no baseline hat.
    let (default_hat, repo_dir) = match app.config() {
        Ok(cfg) => (
            Some(cfg.local.default_hat()),
            app.paths.repo.to_string_lossy().into_owned(),
        ),
        Err(_) => (None, app.paths.repo.to_string_lossy().into_owned()),
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
        default_hat => default_hat,
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
    fn render(default_hat: Option<&str>) -> String {
        let mut jinja = Environment::new();
        jinja.add_filter("sh_quote", |v: String| sh_quote(&v));
        jinja.add_template("init", ZSH_INIT).unwrap();
        jinja
            .get_template("init")
            .unwrap()
            .render(context! {
                default_hat => default_hat,
                repo_dir => "/home/t/.hats/repo",
            })
            .unwrap()
    }

    /// True when zsh accepts the script, or when zsh is not installed to ask.
    /// The syntax check is only meaningful where there is a zsh to run it.
    fn parses_as_zsh(script: &str) -> bool {
        let Ok(zsh) = which::which("zsh") else {
            return true;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("init.zsh");
        std::fs::write(&path, script).unwrap();
        std::process::Command::new(zsh)
            .arg("-n")
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    }

    #[test]
    fn the_integration_script_is_valid_zsh_in_every_variant() {
        for profile in [Some("normal"), None] {
            let script = render(profile);
            assert!(
                parses_as_zsh(&script),
                "invalid zsh for {profile:?}:\n{script}"
            );
        }
    }

    #[test]
    fn it_defines_the_hat_function_and_its_completion() {
        let script = render(Some("normal"));
        assert!(script.contains("hat() {"));
        assert!(script.contains("compdef _hats_hats hat"));
        assert!(script.contains("fzf --prompt='hat > '"));
    }

    #[test]
    fn the_baseline_eval_is_quiet_and_cannot_break_a_shell() {
        let script = render(Some("normal"));
        assert!(script.contains("hats env normal --quiet"));
        assert!(script.contains("2>/dev/null"));
        assert!(script.contains("|| true"), "shell startup must never fail");
    }

    #[test]
    fn without_a_config_there_is_no_baseline_eval() {
        let script = render(None);
        // The `hat` function itself calls `hats env`, so assert on the
        // baseline block specifically: the guard that runs it at shell start.
        assert!(
            !script.contains("if [ -z \"${HATS_HAT:-}\" ]"),
            "an unconfigured machine must not eval a hat at shell start:\n{script}"
        );
        assert!(!script.contains("--quiet"), "{script}");
        // But the function is still defined, so `hat` works after init.
        assert!(script.contains("hat() {"));
    }

    #[test]
    fn the_switch_captures_before_it_evals() {
        let script = render(Some("normal"));
        let capture = script.find("script=$(command hats env").unwrap();
        let evaluate = script.find("eval \"$script\"").unwrap();
        assert!(
            capture < evaluate,
            "must not eval a stream that may fail midway"
        );
    }
}
