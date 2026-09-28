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
use crate::hat::folder::{self, Step};
use crate::hat::{EnvOptions, EnvPlan, aws, azure, coder, github, k9s, kube, terraform};
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
    // `--here` runs on every `cd`, and on most of them nothing changes, so
    // settle that before paying for the configuration.
    let shell = folder::Shell::from_env();
    let step = if args.here {
        match folder::decide(&std::env::current_dir()?, &shell)? {
            Step::Nothing => return Ok(String::new()),
            step => Some(step),
        }
    } else {
        None
    };

    let cfg = app.config()?;
    let secrets = Secrets::load(&app.paths.secrets)?;
    let platform = app.platform()?;

    let opts = EnvOptions {
        no_kube: args.no_kube,
        no_aws: args.no_aws,
        no_colour: args.no_colour,
        reset_only: args.reset,
    };
    let name = match (&step, &args.hat) {
        (Some(Step::Enter { hat, .. }), _) => hat.clone(),
        // The hat to go back to may have been deleted since.
        (Some(Step::Leave { prev }), _) => prev
            .clone()
            .filter(|p| cfg.hats().contains_key(p))
            .unwrap_or_else(|| cfg.local.default_hat()),
        (_, Some(n)) => n.clone(),
        (_, None) => cfg.local.default_hat(),
    };

    // A folder that asks for the hat already on changes only the folder state.
    if let Some(step) = &step
        && shell.hat.as_deref() == Some(name.as_str())
    {
        return Ok(folder_state_only(step));
    }

    let mut plan = EnvPlan::build(&cfg, &name, &secrets, &platform.home, opts)?;

    match &step {
        Some(Step::Enter { file, prev, .. }) => {
            // At the front: HATS_HAT stays the last thing a switch sets.
            if let Some(prev) = prev {
                plan.set
                    .shift_insert(0, folder::PREV_VAR.into(), prev.clone());
            }
            plan.set.shift_insert(
                0,
                folder::FILE_VAR.into(),
                file.to_string_lossy().into_owned(),
            );
        }
        Some(Step::Leave { .. }) => {
            plan.unset
                .extend([folder::FILE_VAR.into(), folder::PREV_VAR.into()]);
            plan.unset.sort();
        }
        _ => {}
    }

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

    // The per-hat config directories must exist before az or gh write their
    // tokens into them, or the first login lands in a missing path.
    if plan.terraform_config_file.is_some() {
        match terraform::isolate(&platform.home, &name) {
            Ok(file) => {
                plan.set.insert(
                    "TF_CLI_CONFIG_FILE".into(),
                    file.to_string_lossy().into_owned(),
                );
            }
            Err(e) => {
                app.ui.detail(format!("terraform isolation skipped: {e:#}"));
                plan.set.shift_remove("TF_CLI_CONFIG_FILE");
            }
        }
    }

    if plan.azure_config_dir.is_some() {
        match azure::isolate(&platform.home, &name) {
            Ok(dir) => {
                plan.set.insert(
                    "AZURE_CONFIG_DIR".into(),
                    dir.to_string_lossy().into_owned(),
                );
            }
            Err(e) => {
                app.ui.detail(format!("azure isolation skipped: {e:#}"));
                plan.set.shift_remove("AZURE_CONFIG_DIR");
            }
        }
    }

    if plan.github_config_dir.is_some() {
        match github::isolate(&platform.home, &name) {
            Ok(dir) => {
                plan.set
                    .insert("GH_CONFIG_DIR".into(), dir.to_string_lossy().into_owned());
            }
            Err(e) => {
                app.ui.detail(format!("github isolation skipped: {e:#}"));
                plan.set.shift_remove("GH_CONFIG_DIR");
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

/// The script for a `--here` step that keeps the hat and moves only the
/// folder state.
fn folder_state_only(step: &Step) -> String {
    match step {
        Step::Enter { file, prev, .. } => {
            let mut out = format!(
                "export {}={}\n",
                folder::FILE_VAR,
                sh_quote(&file.to_string_lossy())
            );
            if let Some(prev) = prev {
                out.push_str(&format!("export {}={}\n", folder::PREV_VAR, sh_quote(prev)));
            }
            out
        }
        Step::Leave { .. } => format!(
            "unset {} {} 2>/dev/null\n",
            folder::FILE_VAR,
            folder::PREV_VAR
        ),
        Step::Nothing => String::new(),
    }
}

pub fn shell_init(app: &mut App, args: &ShellInitArgs) -> Result<()> {
    // shell-init must work before `hats init` has run, so a missing config is
    // not fatal: emit the functions with no baseline hat.
    let (default_hat, vscode_profiles) = match app.config() {
        // The `code` function is only worth defining where hats manages the
        // editor, and it costs a `hats` call per `code` where it is defined.
        Ok(cfg) => (Some(cfg.local.default_hat()), cfg.vscode_enabled()),
        Err(_) => (None, false),
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
        vscode_profiles => vscode_profiles,
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
        render_with(default_hat, true)
    }

    fn render_with(default_hat: Option<&str>, vscode_profiles: bool) -> String {
        let mut jinja = Environment::new();
        jinja.add_filter("sh_quote", |v: String| sh_quote(&v));
        jinja.add_template("init", ZSH_INIT).unwrap();
        jinja
            .get_template("init")
            .unwrap()
            .render(context! {
                default_hat => default_hat,
                vscode_profiles => vscode_profiles,
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
            for vscode in [true, false] {
                let script = render_with(profile, vscode);
                assert!(
                    parses_as_zsh(&script),
                    "invalid zsh for {profile:?}, vscode {vscode}:\n{script}"
                );
            }
        }
    }

    #[test]
    fn the_code_function_is_defined_only_where_hats_manages_the_editor() {
        let script = render_with(Some("normal"), true);
        assert!(script.contains("code() {"), "{script}");
        assert!(
            script.contains("hats hat vscode-profile --ensure"),
            "{script}"
        );
        // A profile hats does not know about must not be invented by `code`.
        assert!(script.contains("command code \"$@\""), "{script}");

        let without = render_with(Some("normal"), false);
        assert!(!without.contains("code() {"), "{without}");
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
    fn the_cd_hook_follows_dot_hat_files_and_cannot_break_a_shell() {
        let script = render(Some("normal"));
        assert!(script.contains("add-zsh-hook chpwd _hats_here"), "{script}");
        assert!(
            script.contains("script=$(command hats env --here --quiet) || return 0"),
            "capture first, and a failure must not fail the cd:\n{script}"
        );
        // A terminal opened inside a tree wears its hat from the start, quietly.
        assert!(script.contains("_hats_here quiet 2>/dev/null"), "{script}");

        // An unconfigured machine must not pay for a `hats` call on every cd.
        assert!(!render(None).contains("_hats_here"));
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
