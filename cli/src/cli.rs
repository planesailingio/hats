//! The command-line surface.

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "hats",
    version,
    about = "One laptop, many hats: dotfiles and per-shell client hats",
    long_about = None,
    propagate_version = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Args)]
pub struct GlobalOpts {
    /// Use this directory instead of ~/.hats
    #[arg(long, global = true, value_name = "DIR", env = "HATS_HOME")]
    pub hats_home: Option<PathBuf>,

    /// Never prompt: take every default, or the answers file
    #[arg(long, global = true)]
    pub non_interactive: bool,

    /// YAML file of recorded wizard answers (implies --non-interactive)
    #[arg(long, global = true, value_name = "FILE")]
    pub answers: Option<PathBuf>,

    /// Disable coloured output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Show more detail (repeatable)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Set up hats on this machine: identity, hats, features, secrets
    Init(InitArgs),

    /// Show the version of this binary
    Version(VersionArgs),

    /// Check that this machine has what hats needs
    Doctor(DoctorArgs),

    /// Inspect and manage hats
    Hat(HatArgs),

    /// Fetch, inspect or clear the secrets hats hands to hats
    Secrets(SecretsArgs),

    /// Check the hat switcher actually works on this machine
    Test(TestArgs),

    /// Print the shell code that switches this shell to a hat
    Env(EnvArgs),

    /// Print the shell integration: the `hat` switcher and its completion
    ShellInit(ShellInitArgs),

    /// Generate a shell completion script
    Completions(CompletionsArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Re-run the wizard over an existing configuration
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct VersionArgs {
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct HatArgs {
    #[command(subcommand)]
    pub command: Option<HatCommand>,
}

#[derive(Debug, Subcommand)]
pub enum HatCommand {
    /// List the hats configured on this machine
    List {
        /// One name per line, for shell completion and fzf
        #[arg(long)]
        plain: bool,
    },
    /// Show one hat with its inheritance folded in
    Show {
        name: String,
        /// Machine-readable output
        #[arg(long)]
        json: bool,
    },
    /// Print the active hat
    Current {
        /// One line: hat, git identity, AWS profile, kube context
        #[arg(long)]
        summary: bool,
    },
    /// Show the variables hats unsets before every switch
    ResetList,
    /// Create every missing file hats scaffolds: the base skeletons
    /// (~/.gitconfig, ~/.ssh/config, ~/.terraformrc, ~/.tofurc), each hat's
    /// per-tool files, and VS Code profiles
    Sync,
    /// Add a hat to ~/.hats/config.yaml and create its per-hat files
    Create(HatCreateArgs),
    /// Remove a hat and move every file it owns, edited or not, to the backups
    Delete {
        name: String,
        /// Do not ask for confirmation
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Report whether VS Code has a profile for this hat, for the `code`
    /// shell function. Exit 0 if it has, 1 if not.
    VscodeProfile {
        name: String,
        /// First register any hat's missing profile, if VS Code is closed
        #[arg(long)]
        ensure: bool,
    },
}

#[derive(Debug, Args)]
pub struct HatCreateArgs {
    /// Name for the hat: letters, digits, `-`, `_` and `.`
    pub name: String,

    /// Hat to inherit from (default: ask, suggesting the default hat)
    #[arg(long, value_name = "HAT", conflicts_with = "no_inherit")]
    pub inherits: Option<String>,

    /// Inherit nothing but the machine identity
    #[arg(long)]
    pub no_inherit: bool,

    /// Git author name, if not the inherited one
    #[arg(long, value_name = "NAME")]
    pub git_name: Option<String>,

    /// Git author email, if not the inherited one
    #[arg(long, value_name = "EMAIL")]
    pub git_email: Option<String>,

    /// Context to select in the hat's own kubeconfig
    #[arg(long, value_name = "CONTEXT")]
    pub kube_context: Option<String>,

    /// Terminal background tint, e.g. "#0d2a52"
    #[arg(long, alias = "color", value_name = "HEX")]
    pub colour: Option<String>,

    /// One line on what the hat is for
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,
}

#[derive(Debug, Args)]
pub struct TestArgs {}

#[derive(Debug, Args)]
pub struct SecretsArgs {
    #[command(subcommand)]
    pub command: SecretsCommand,
}

#[derive(Debug, Subcommand)]
pub enum SecretsCommand {
    /// Pull secrets from the configured provider into ~/.hats/secrets.yaml
    Fetch,
    /// Show which secrets are set and which are still missing (never values)
    Status,
    /// Delete the fetched secrets and the credential envelope
    Clear,
    /// Generate a YubiKey PIV key and seal the vault credentials to it
    EnrolYubikey {
        /// Retired PIV slot to use, numbered 1-20 (default: 1)
        #[arg(long)]
        slot: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct EnvArgs {
    /// Hat to switch to (default: the configured default hat)
    pub hat: Option<String>,

    /// Choose the hat from the nearest `.hat` file at or above the current
    /// directory, and print nothing when that changes nothing. Used by the
    /// shell's `cd` hook.
    #[arg(long, conflicts_with = "hat")]
    pub here: bool,

    /// Do not touch the kubeconfig
    #[arg(long)]
    pub no_kube: bool,

    /// Do not touch the AWS config or credentials
    #[arg(long)]
    pub no_aws: bool,

    /// Do not emit the terminal tint
    #[arg(long)]
    pub no_colour: bool,

    /// Emit only the unset block
    #[arg(long)]
    pub reset: bool,

    /// Never fail: on error print nothing to stdout and exit 0. Used by the
    /// baseline eval in shell startup, which must not break a login shell.
    #[arg(long)]
    pub quiet: bool,
}

#[derive(Debug, Args)]
pub struct ShellInitArgs {
    /// Shell to emit for (currently only zsh)
    #[arg(default_value = "zsh")]
    pub shell: String,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to generate for
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_tree_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn global_flags_are_accepted_after_the_subcommand() {
        let cli = Cli::try_parse_from(["hats", "version", "--json", "-vv"]).unwrap();
        assert_eq!(cli.global.verbose, 2);
        assert!(matches!(
            cli.command,
            Command::Version(VersionArgs { json: true })
        ));
    }

    #[test]
    fn hats_home_can_come_from_a_flag() {
        let cli = Cli::try_parse_from(["hats", "--hats-home", "/tmp/h", "doctor"]).unwrap();
        assert_eq!(cli.global.hats_home.unwrap().to_str().unwrap(), "/tmp/h");
    }

    #[test]
    fn hat_defaults_to_no_subcommand() {
        let cli = Cli::try_parse_from(["hats", "hat"]).unwrap();
        match cli.command {
            Command::Hat(a) => assert!(a.command.is_none()),
            other => panic!("expected hat, got {other:?}"),
        }
    }

    #[test]
    fn hat_create_and_delete_parse() {
        let cli = Cli::try_parse_from([
            "hats",
            "hat",
            "create",
            "globex",
            "--git-email",
            "j@globex.example",
            "--color",
            "#0d2a52",
        ])
        .unwrap();
        match cli.command {
            Command::Hat(HatArgs {
                command: Some(HatCommand::Create(a)),
            }) => {
                assert_eq!(a.name, "globex");
                assert_eq!(a.git_email.as_deref(), Some("j@globex.example"));
                assert_eq!(a.colour.as_deref(), Some("#0d2a52"), "--color is an alias");
            }
            other => panic!("expected hat create, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["hats", "hat", "delete", "globex", "-y"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Hat(HatArgs {
                command: Some(HatCommand::Delete { yes: true, .. })
            })
        ));

        assert!(
            Cli::try_parse_from([
                "hats",
                "hat",
                "create",
                "x",
                "--inherits",
                "a",
                "--no-inherit"
            ])
            .is_err(),
            "--inherits and --no-inherit contradict each other"
        );
    }

    #[test]
    fn env_here_takes_no_hat_name() {
        let cli = Cli::try_parse_from(["hats", "env", "--here", "--quiet"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Env(EnvArgs {
                here: true,
                hat: None,
                ..
            })
        ));

        assert!(
            Cli::try_parse_from(["hats", "env", "acme", "--here"]).is_err(),
            "--here chooses the hat, so naming one contradicts it"
        );
    }

    #[test]
    fn hat_sync_parses() {
        let cli = Cli::try_parse_from(["hats", "hat", "sync"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Hat(HatArgs {
                command: Some(HatCommand::Sync)
            })
        ));
    }
}
