//! Rendering an [`EnvPlan`](super::EnvPlan) as shell code.
//!
//! The output is `eval`ed by the caller's shell, so two rules matter above all:
//! every value is quoted, and the script must be inert when something goes
//! wrong. `hats env` writes nothing to stdout on failure, because a partial
//! script evaluated by a login shell is far worse than no script.

use super::EnvPlan;

/// A shell dialect hats can emit for.
pub trait ShellEmitter {
    fn name(&self) -> &'static str;
    fn emit(&self, plan: &EnvPlan) -> String;
}

/// POSIX single-quoting: wrap in single quotes and replace each embedded quote
/// with `'\''`. Safe for every byte, including newlines and `$`.
pub fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@%_+=:,./-".contains(&b))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// zsh, and by construction bash and any POSIX shell: the emitted code uses no
/// zsh-specific syntax.
#[derive(Debug, Clone, Copy, Default)]
pub struct Zsh;

impl ShellEmitter for Zsh {
    fn name(&self) -> &'static str {
        "zsh"
    }

    fn emit(&self, plan: &EnvPlan) -> String {
        let mut out = String::new();

        out.push_str(&format!("# hats: hat {}\n", plan.hat));

        // Unset before you set. `2>/dev/null` because unsetting a variable that
        // was never set is noisy in some shells and harmless in all of them.
        if !plan.unset.is_empty() {
            out.push_str("unset ");
            out.push_str(&plan.unset.join(" "));
            out.push_str(" 2>/dev/null\n");
        }

        for (k, v) in &plan.set {
            out.push_str(&format!("export {k}={}\n", sh_quote(v)));
        }

        // Guarded so re-sourcing a hat cannot grow PATH without bound.
        for p in &plan.path_prepend {
            let q = sh_quote(p);
            out.push_str(&format!(
                "case \":$PATH:\" in *:{q}:*) ;; *) PATH={q}:\"$PATH\"; export PATH;; esac\n"
            ));
        }

        if let Some(colour) = &plan.colour {
            out.push_str(&colour_line(colour));
        }

        out
    }
}

/// Tint this terminal, or this tmux pane.
///
/// Inside tmux only the current pane is recoloured, so two panes can hold two
/// hats, matching the per-shell kubeconfig property. Outside tmux, OSC 11
/// recolours the window; terminals that do not understand it ignore it.
/// Guarded on a tty so the escape never lands in a pipe or a log.
fn colour_line(colour: &str) -> String {
    if colour == "reset" {
        return "[ -t 1 ] && { [ -n \"${TMUX:-}\" ] && tmux select-pane -P 'bg=default' \
                >/dev/null 2>&1 || printf '\\033]111\\007'; }\n"
            .to_string();
    }
    let q = sh_quote(colour);
    format!(
        "[ -t 1 ] && {{ [ -n \"${{TMUX:-}}\" ] && tmux select-pane -P bg={q} >/dev/null 2>&1 \
         || printf '\\033]11;%s\\007' {q}; }}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::super::{EnvOptions, EnvPlan};
    use super::*;

    fn emit(name: &str) -> String {
        let cfg = config(PROFILES);
        let s = secrets(&[("git_signing_key", "SIGNKEY"), ("jira_token", "tok-123")]);
        let plan = EnvPlan::build(
            &cfg,
            name,
            &s,
            std::path::Path::new("/home/t"),
            EnvOptions::default(),
        )
        .unwrap();
        Zsh.emit(&plan)
    }

    #[test]
    fn simple_values_are_left_unquoted_and_awkward_ones_are_quoted() {
        assert_eq!(sh_quote("simple"), "simple");
        assert_eq!(sh_quote("eu-west-2"), "eu-west-2");
        assert_eq!(sh_quote("/home/t/.kube/config"), "/home/t/.kube/config");
        assert_eq!(sh_quote("code --wait"), "'code --wait'");
        assert_eq!(sh_quote(""), "''");
    }

    /// The property that matters: nothing inside a value can escape the quotes
    /// and execute.
    #[test]
    fn dangerous_values_cannot_break_out() {
        assert_eq!(sh_quote("$(rm -rf /)"), "'$(rm -rf /)'");
        assert_eq!(sh_quote("`whoami`"), "'`whoami`'");
        assert_eq!(sh_quote("a'b"), r"'a'\''b'");
        assert_eq!(sh_quote("two\nlines"), "'two\nlines'");
        assert_eq!(sh_quote("$HOME"), "'$HOME'");
    }

    /// A token with an apostrophe in it must survive a real shell round trip.
    #[test]
    fn a_quoted_value_survives_evaluation_by_a_real_shell() {
        let nasty = "it's $HOME `x` \"q\"";
        let script = format!("printf %s {}", sh_quote(nasty));
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), nasty);
    }

    #[test]
    fn the_unset_block_comes_before_any_export() {
        let script = emit("acme");
        let unset_at = script.find("unset ").expect("no unset block");
        let export_at = script.find("export ").expect("no exports");
        assert!(unset_at < export_at, "reset must precede set:\n{script}");
    }

    #[test]
    fn the_profile_marker_is_the_last_export() {
        let script = emit("acme");
        let exports: Vec<&str> = script
            .lines()
            .filter(|l| l.starts_with("export "))
            .collect();
        assert_eq!(*exports.last().unwrap(), "export HATS_HAT=acme");
    }

    #[test]
    fn path_prepends_are_idempotent() {
        let script = emit("acme");
        assert!(
            script.contains(r#"case ":$PATH:" in *:/home/t/.tenv/bin:*) ;;"#),
            "PATH guard missing:\n{script}"
        );
    }

    #[test]
    fn the_tint_is_applied_per_pane_under_tmux() {
        let script = emit("acme");
        assert!(script.contains("tmux select-pane -P bg='#331420'"));
        assert!(script.contains(r"printf '\033]11;%s\007' '#331420'"));
        assert!(script.contains("[ -t 1 ]"), "must not tint a non-tty");
    }

    #[test]
    fn reset_restores_the_terminal_default() {
        let line = colour_line("reset");
        assert!(line.contains("bg=default"));
        assert!(line.contains(r"\033]111\007"));
    }

    /// The emitted script must be valid shell, checked by a real shell rather
    /// than by eye.
    ///
    /// `sh` is always present. `zsh` is checked when installed and skipped when
    /// not, so a runner without it reports honestly instead of failing on a
    /// missing interpreter.
    #[test]
    fn the_emitted_script_parses_in_sh_and_zsh() {
        for hat in ["normal", "acme", "plain"] {
            let script = emit(hat);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("env.sh");
            std::fs::write(&path, &script).unwrap();
            for shell in ["sh", "zsh"] {
                if which::which(shell).is_err() {
                    continue;
                }
                let out = std::process::Command::new(shell)
                    .arg("-n")
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "{shell} rejected the script for {hat}:\n{script}\n{}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
    }

    /// End to end: evaluate the script in a real shell and read the result
    /// back, which is the only way to know the switch actually works.
    #[test]
    fn evaluating_the_script_produces_the_expected_environment() {
        let script = emit("acme");
        let probe = format!(
            "{script}\nprintf '%s|%s|%s|%s\\n' \
             \"$HATS_HAT\" \"$GIT_AUTHOR_EMAIL\" \"$AWS_PROFILE\" \"$JIRA_API_TOKEN\""
        );
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&probe)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "acme|jane@acme.example|acme-aws|tok-123"
        );
    }

    /// The leak fix, proven in a real shell: start with the previous client's
    /// variables set, switch to a hat that sets none of them, and confirm
    /// they are gone.
    #[test]
    fn switching_profiles_clears_the_previous_clients_variables() {
        let script = emit("normal");
        let probe = format!(
            "export JIRA_API_TOKEN=leaked AWS_PROFILE=acme-aws JIRA_EMAIL=old\n\
             {script}\n\
             printf '[%s][%s][%s]\\n' \"$JIRA_API_TOKEN\" \"$JIRA_EMAIL\" \"$AWS_PROFILE\""
        );
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&probe)
            .output()
            .unwrap();
        // AWS_PROFILE is set by `normal`; the acme-only variables are not.
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[][][default]");
    }
}
