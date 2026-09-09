//! Subcommand implementations. Each is thin: parse-free, it takes the already
//! validated arguments and calls into the library modules.

pub mod doctor;
pub mod env;
pub mod hat;
pub mod init;
pub mod plan;
pub mod secrets;
pub mod selftest;
pub mod tools;
pub mod update;
pub mod version;

use anyhow::Result;
use clap::CommandFactory;

use crate::app::App;
use crate::cli::{Cli, Command, CompletionsArgs};

/// Run the parsed command. Returns the process exit code.
pub fn dispatch(app: &mut App, command: &Command) -> Result<i32> {
    match command {
        Command::Init(args) => {
            init::run(app, args)?;
            Ok(0)
        }
        Command::Update(args) => match update::run(app, args)? {
            Some(update::CheckExit(code)) => Ok(code),
            None => Ok(0),
        },
        Command::Version(args) => {
            version::run(app, args)?;
            Ok(0)
        }
        Command::Doctor(args) => {
            doctor::run(app, args)?;
            Ok(0)
        }
        Command::Hat(args) => {
            hat::run(app, args)?;
            Ok(0)
        }
        Command::Plan(args) => plan::plan(app, args),
        Command::Apply(args) => plan::apply(app, args),
        Command::Diff(args) => plan::diff(app, args),
        Command::Render(args) => plan::render(app, args),
        Command::Brew(args) => tools::brew(app, args),
        Command::Hooks(args) => tools::hooks(app, args),
        Command::Lint(args) => tools::lint(app, args),
        Command::Secrets(args) => secrets::run(app, args),
        Command::Test(args) => selftest::run(app, args),
        Command::Env(args) => {
            env::env(app, args)?;
            Ok(0)
        }
        Command::ShellInit(args) => {
            env::shell_init(app, args)?;
            Ok(0)
        }
        Command::Completions(args) => {
            completions(args);
            Ok(0)
        }
    }
}

fn completions(args: &CompletionsArgs) {
    let mut cmd = Cli::command();
    clap_complete::generate(args.shell, &mut cmd, "hats", &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completions_generate_for_every_shell_we_advertise() {
        for shell in [
            clap_complete::Shell::Zsh,
            clap_complete::Shell::Bash,
            clap_complete::Shell::Fish,
        ] {
            let mut cmd = Cli::command();
            let mut out = Vec::new();
            clap_complete::generate(shell, &mut cmd, "hats", &mut out);
            let text = String::from_utf8(out).unwrap();
            assert!(text.contains("hats"), "{shell:?} produced nothing useful");
        }
    }
}
