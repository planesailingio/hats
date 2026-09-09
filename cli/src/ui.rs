//! Output and prompting.
//!
//! Every question hats asks goes through [`Prompter`]. That is what makes
//! `--non-interactive` total: swap the implementation and the whole wizard
//! runs unattended, from defaults or from a recorded answers file, with no
//! second code path to drift.
//!
//! Questions are addressed by a stable dotted `key` (`groups.shell`,
//! `profile.1.email`). The key is what an answers file matches on, so
//! rewording a prompt never invalidates a fixture.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use owo_colors::{OwoColorize, Stream};

/// Anything that can answer the wizard's questions.
pub trait Prompter {
    fn confirm(&mut self, key: &str, message: &str, default: bool) -> Result<bool>;
    fn text(&mut self, key: &str, message: &str, default: Option<&str>) -> Result<String>;
    fn select(&mut self, key: &str, message: &str, options: &[&str]) -> Result<String>;
    /// True when the user is actually present, so callers can skip niceties
    /// (spinners, "press enter to continue") rather than branch on a flag.
    fn is_interactive(&self) -> bool {
        false
    }
}

/// Asks the human, via inquire.
pub struct Interactive;

impl Prompter for Interactive {
    fn confirm(&mut self, _key: &str, message: &str, default: bool) -> Result<bool> {
        inquire::Confirm::new(message)
            .with_default(default)
            .prompt()
            .context("prompt cancelled")
    }

    fn text(&mut self, _key: &str, message: &str, default: Option<&str>) -> Result<String> {
        let mut q = inquire::Text::new(message);
        if let Some(d) = default {
            q = q.with_default(d);
        }
        q.prompt().context("prompt cancelled")
    }

    fn select(&mut self, _key: &str, message: &str, options: &[&str]) -> Result<String> {
        inquire::Select::new(message, options.to_vec())
            .prompt()
            .map(str::to_owned)
            .context("prompt cancelled")
    }

    fn is_interactive(&self) -> bool {
        true
    }
}

/// Answers from a recorded YAML file, falling back to each question's default.
///
/// The file is a flat map so fixtures stay readable:
///
/// ```yaml
/// answers:
///   groups.shell: true
///   profile.1.name: normal
/// ```
#[derive(Debug, Default)]
pub struct Answers {
    values: BTreeMap<String, serde_yaml_ng::Value>,
    /// Keys that were asked for but not present, reported by `hats init` so a
    /// half-written fixture is obvious rather than silently taking defaults.
    pub missed: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AnswersFile {
    #[serde(default)]
    answers: BTreeMap<String, serde_yaml_ng::Value>,
}

impl Answers {
    /// Take every default without consulting a file.
    pub fn defaults_only() -> Self {
        Self::default()
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading answers file {}", path.display()))?;
        let parsed: AnswersFile = serde_yaml_ng::from_str(&text)
            .with_context(|| format!("parsing answers file {}", path.display()))?;
        Ok(Self {
            values: parsed.answers,
            missed: Vec::new(),
        })
    }

    fn lookup(&mut self, key: &str) -> Option<serde_yaml_ng::Value> {
        match self.values.get(key) {
            Some(v) => Some(v.clone()),
            None => {
                self.missed.push(key.to_owned());
                None
            }
        }
    }
}

impl Prompter for Answers {
    fn confirm(&mut self, key: &str, _message: &str, default: bool) -> Result<bool> {
        match self.lookup(key) {
            Some(serde_yaml_ng::Value::Bool(b)) => Ok(b),
            Some(other) => bail!("answer for `{key}` must be true or false, got {other:?}"),
            None => Ok(default),
        }
    }

    fn text(&mut self, key: &str, _message: &str, default: Option<&str>) -> Result<String> {
        match self.lookup(key) {
            Some(serde_yaml_ng::Value::String(s)) => Ok(s),
            Some(serde_yaml_ng::Value::Number(n)) => Ok(n.to_string()),
            Some(other) => bail!("answer for `{key}` must be a string, got {other:?}"),
            None => default
                .map(str::to_owned)
                .with_context(|| format!("no answer for `{key}` and the question has no default")),
        }
    }

    fn select(&mut self, key: &str, message: &str, options: &[&str]) -> Result<String> {
        let chosen = match self.lookup(key) {
            Some(serde_yaml_ng::Value::String(s)) => s,
            Some(other) => bail!("answer for `{key}` must be a string, got {other:?}"),
            None => options
                .first()
                .map(|s| (*s).to_owned())
                .with_context(|| format!("no answer for `{key}` and no options to default to"))?,
        };
        if !options.contains(&chosen.as_str()) {
            bail!(
                "answer for `{key}` ({chosen}) is not one of the options for \"{message}\": {}",
                options.join(", ")
            );
        }
        Ok(chosen)
    }
}

/// Refuses to answer. Used where a prompt would be a bug (a non-interactive
/// command that should never need input).
pub struct Deny;

impl Prompter for Deny {
    fn confirm(&mut self, key: &str, message: &str, _default: bool) -> Result<bool> {
        bail!("this command cannot prompt (`{key}`: {message})")
    }
    fn text(&mut self, key: &str, message: &str, _default: Option<&str>) -> Result<String> {
        bail!("this command cannot prompt (`{key}`: {message})")
    }
    fn select(&mut self, key: &str, message: &str, _options: &[&str]) -> Result<String> {
        bail!("this command cannot prompt (`{key}`: {message})")
    }
}

/// Terminal output. Colour is decided once, here, honouring `NO_COLOR` and
/// whether stdout is a tty.
pub struct Ui {
    pub prompter: Box<dyn Prompter>,
    colour: bool,
    verbose: u8,
}

impl Ui {
    pub fn new(prompter: Box<dyn Prompter>, no_color: bool, verbose: u8) -> Self {
        use std::io::IsTerminal;
        let colour =
            !no_color && std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();
        Self {
            prompter,
            colour,
            verbose,
        }
    }

    pub fn colour(&self) -> bool {
        self.colour
    }

    /// A normal line of output.
    pub fn say(&self, msg: impl AsRef<str>) {
        println!("{}", msg.as_ref());
    }

    /// A step that succeeded.
    pub fn ok(&self, msg: impl AsRef<str>) {
        let mark = if self.colour {
            "✓"
                .if_supports_color(Stream::Stdout, |t| t.green())
                .to_string()
        } else {
            "ok".to_string()
        };
        println!("{mark} {}", msg.as_ref());
    }

    /// Something the user should notice but which is not fatal.
    pub fn warn(&self, msg: impl AsRef<str>) {
        let mark = if self.colour {
            "!".if_supports_color(Stream::Stderr, |t| t.yellow())
                .to_string()
        } else {
            "warning:".to_string()
        };
        eprintln!("{mark} {}", msg.as_ref());
    }

    /// Detail shown only with `-v`.
    pub fn detail(&self, msg: impl AsRef<str>) {
        if self.verbose > 0 {
            eprintln!("  {}", msg.as_ref());
        }
    }

    /// A section heading in the wizard and in reports.
    pub fn heading(&self, msg: impl AsRef<str>) {
        if self.colour {
            println!(
                "\n{}",
                msg.as_ref().if_supports_color(Stream::Stdout, |t| t.bold())
            );
        } else {
            println!("\n{}", msg.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers_from(yaml: &str) -> Answers {
        let parsed: AnswersFile = serde_yaml_ng::from_str(yaml).unwrap();
        Answers {
            values: parsed.answers,
            missed: Vec::new(),
        }
    }

    #[test]
    fn recorded_answers_win_over_defaults() {
        let mut a = answers_from("answers:\n  groups.shell: false\n  profile.1.name: work\n");
        assert!(!a.confirm("groups.shell", "shell?", true).unwrap());
        assert_eq!(
            a.text("profile.1.name", "name?", Some("normal")).unwrap(),
            "work"
        );
    }

    #[test]
    fn missing_answers_fall_back_to_defaults_and_are_recorded() {
        let mut a = Answers::defaults_only();
        assert!(a.confirm("groups.git", "git?", true).unwrap());
        assert_eq!(
            a.text("identity.name", "name?", Some("Jane")).unwrap(),
            "Jane"
        );
        assert_eq!(a.missed, vec!["groups.git", "identity.name"]);
    }

    #[test]
    fn a_text_answer_with_no_default_is_an_error_not_an_empty_string() {
        let mut a = Answers::defaults_only();
        assert!(a.text("identity.email", "email?", None).is_err());
    }

    #[test]
    fn wrong_types_are_rejected() {
        let mut a = answers_from("answers:\n  groups.shell: yes-please\n");
        assert!(a.confirm("groups.shell", "shell?", true).is_err());
    }

    #[test]
    fn select_rejects_an_answer_outside_the_options() {
        let mut a = answers_from("answers:\n  secrets.provider: sops\n");
        let err = a
            .select("secrets.provider", "provider?", &["bitwarden", "none"])
            .unwrap_err();
        assert!(err.to_string().contains("not one of the options"));
    }

    #[test]
    fn select_defaults_to_the_first_option() {
        let mut a = Answers::defaults_only();
        assert_eq!(
            a.select("secrets.provider", "provider?", &["bitwarden", "none"])
                .unwrap(),
            "bitwarden"
        );
    }

    #[test]
    fn deny_refuses_every_kind_of_question() {
        let mut d = Deny;
        assert!(d.confirm("k", "m", true).is_err());
        assert!(d.text("k", "m", Some("x")).is_err());
        assert!(d.select("k", "m", &["a"]).is_err());
    }
}
