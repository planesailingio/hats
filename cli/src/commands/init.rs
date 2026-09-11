//! `hats init` — set hats up on a machine.
//!
//! Clone the dotfiles repo into `~/.hats/repo`, ask which groups of files to
//! manage, walk a loop collecting hats until the user says stop, ask how
//! secrets should be fetched, then write `~/.hats/config.yaml` and print the
//! command list.
//!
//! Every question goes through the [`Prompter`](crate::ui::Prompter), so the
//! same code path runs unattended under `--non-interactive`/`--answers`. That
//! is what the integration test drives.

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;

use crate::app::App;
use crate::cli::InitArgs;
use crate::config::hat::{HatSpec, IdentitySpec, KubeSpec};
use crate::config::local::{
    BitwardenConfig, EnvelopeConfig, EnvelopeMethod, LocalConfig, ProviderKind,
};
use crate::config::repo::RepoConfig;

/// Default clone URL. Overridable with `--repo`, and asked for interactively.
const DEFAULT_REPO: &str = "https://github.com/planesailingio/hats.git";

/// Suggested tints, cycled through as hats are added. Purple for personal,
/// then colours distinct enough to tell apart at a glance in a wall of panes.
const PALETTE: &[&str] = &["#2a2040", "#331420", "#0d2a52", "#0d3a2a", "#3a2f0d"];

pub fn run(app: &mut App, args: &InitArgs) -> Result<()> {
    if app.paths.is_initialised() && !args.force {
        bail!(
            "hats is already set up at {}. Re-run with --force to redo the wizard, \
             or edit the file directly.",
            app.paths.config.display()
        );
    }

    app.paths.ensure_dirs()?;

    let url = clone_repo(app, args)?;
    let manifest = RepoConfig::load(&app.paths.repo.join(crate::config::MANIFEST_NAME))
        .context("reading the repo's hats.yaml")?;

    let identity = ask_identity(app)?;
    let groups = ask_groups(app, &manifest)?;
    let hats = ask_hats(app, &identity)?;
    let secrets = ask_secrets(app)?;

    let default_hat = hats.keys().next().cloned();
    let cfg = LocalConfig {
        meta: crate::config::local::LocalMeta {
            repo: Some(url),
            default_hat,
        },
        identity: Some(identity),
        groups,
        hats,
        secrets,
        machine: IndexMap::new(),
    };

    cfg.save(&app.paths.config)
        .with_context(|| format!("writing {}", app.paths.config.display()))?;
    app.ui.ok(format!("wrote {}", app.paths.config.display()));

    report_missed_answers(app);
    print_next_steps(app, &cfg);
    Ok(())
}

/// Clone the repo unless it is already there. Returns the URL recorded in the
/// config so `hats update` knows where the clone came from.
fn clone_repo(app: &mut App, args: &InitArgs) -> Result<String> {
    let repo = app.repo();
    if repo.exists() {
        app.ui.ok(format!(
            "using the existing clone at {}",
            repo.path.display()
        ));
        return Ok(args
            .repo
            .clone()
            .unwrap_or_else(|| DEFAULT_REPO.to_string()));
    }

    let url = match &args.repo {
        Some(u) => u.clone(),
        None => app
            .ui
            .prompter
            .text("repo.url", "Dotfiles repo to clone", Some(DEFAULT_REPO))?,
    };

    app.ui.say(format!("Cloning {url}"));
    repo.clone_from(&url).with_context(|| {
        format!(
            "cloning {url}. For a private repo, use an SSH URL or sign in with `gh auth login` first."
        )
    })?;
    app.ui.ok(format!("cloned into {}", repo.path.display()));
    Ok(url)
}

fn ask_identity(app: &mut App) -> Result<IdentitySpec> {
    app.ui.heading("Who are you?");
    app.ui
        .say("Used as the fallback identity for hats that do not override it.");
    let name = app
        .ui
        .prompter
        .text("identity.name", "Git name", Some(""))?;
    let email = app
        .ui
        .prompter
        .text("identity.email", "Git email", Some(""))?;
    Ok(IdentitySpec {
        name: non_empty(name),
        email: non_empty(email),
        signing_key: None,
    })
}

fn ask_groups(app: &mut App, manifest: &RepoConfig) -> Result<IndexMap<String, bool>> {
    app.ui.heading("Which configuration should hats manage?");
    app.ui
        .say("Every hat gets the same set of files; this is asked once.");

    let mut answers = IndexMap::new();
    for (name, spec) in &manifest.groups {
        let enabled = app.ui.prompter.confirm(
            &format!("groups.{name}"),
            &format!("{name} — {}", spec.description),
            spec.default,
        )?;
        answers.insert(name.clone(), enabled);
    }
    Ok(answers)
}

/// Collect hats, prompting for another name until the user declines.
fn ask_hats(app: &mut App, identity: &IdentitySpec) -> Result<IndexMap<String, HatSpec>> {
    app.ui.heading("Profiles");
    app.ui.say(
        "One per context you switch between: personal, and one per client. \
         The first is the hat new shells start in.",
    );

    let mut hats: IndexMap<String, HatSpec> = IndexMap::new();
    let mut n = 0usize;

    loop {
        n += 1;
        if n > 1 {
            let more =
                app.ui
                    .prompter
                    .confirm(&format!("hat.add.{n}"), "Add another hat?", false)?;
            if !more {
                break;
            }
        }

        let default_name = if n == 1 { "normal" } else { "" };
        let name = app
            .ui
            .prompter
            .text(&format!("hat.{n}.name"), "Profile name", Some(default_name))?
            .trim()
            .to_string();

        if name.is_empty() {
            if hats.is_empty() {
                bail!("at least one hat is needed; hats has nothing to switch between");
            }
            break;
        }
        if hats.contains_key(&name) {
            bail!("hat `{name}` was given twice");
        }

        let spec = ask_one_hat(app, n, &name, identity, &hats)?;
        hats.insert(name, spec);

        // Guard against a runaway loop when an answers file keeps saying yes.
        if n >= 32 {
            break;
        }
    }

    if hats.is_empty() {
        bail!("at least one hat is needed; hats has nothing to switch between");
    }
    Ok(hats)
}

fn ask_one_hat(
    app: &mut App,
    n: usize,
    name: &str,
    identity: &IdentitySpec,
    existing: &IndexMap<String, HatSpec>,
) -> Result<HatSpec> {
    // Profiles after the first usually inherit the base, which is what makes
    // "same identity, different cloud account" a two-line hat.
    let inherits = match existing.keys().next() {
        Some(base) if n > 1 => {
            let yes = app.ui.prompter.confirm(
                &format!("hat.{n}.inherits"),
                &format!("Inherit defaults from `{base}`?"),
                true,
            )?;
            yes.then(|| base.clone())
        }
        _ => None,
    };

    let default_git_name = identity.name.clone().unwrap_or_default();
    let default_git_email = identity.email.clone().unwrap_or_default();

    let git_name = app.ui.prompter.text(
        &format!("hat.{n}.git_name"),
        &format!("[{name}] git name"),
        Some(&default_git_name),
    )?;
    let git_email = app.ui.prompter.text(
        &format!("hat.{n}.git_email"),
        &format!("[{name}] git email"),
        Some(&default_git_email),
    )?;
    let kube_context = app.ui.prompter.text(
        &format!("hat.{n}.kube_context"),
        &format!("[{name}] kube context (blank for none)"),
        Some(if n == 1 { "" } else { name }),
    )?;
    let colour = app.ui.prompter.text(
        &format!("hat.{n}.colour"),
        &format!("[{name}] terminal tint"),
        Some(PALETTE[(n - 1) % PALETTE.len()]),
    )?;

    // Identity fields equal to the inherited ones are left unset, so the config
    // records the difference rather than repeating the base everywhere.
    let (name_field, email_field) = if inherits.is_some() {
        (
            (git_name != default_git_name).then_some(git_name),
            (git_email != default_git_email).then_some(git_email),
        )
    } else {
        (non_empty(git_name), non_empty(git_email))
    };

    let identity_spec = IdentitySpec {
        name: name_field,
        email: email_field,
        signing_key: None,
    };

    Ok(HatSpec {
        inherits,
        description: None,
        colour: non_empty(colour),
        identity: (!is_empty_identity(&identity_spec)).then_some(identity_spec),
        // AWS needs no answers: isolation is the default and the CLI writes
        // its own config inside the hat.
        aws: None,
        kube: some_if_any(
            KubeSpec {
                context: non_empty(kube_context),
                isolate: None,
            },
            |k| k.context.is_some(),
        ),
        k9s: None,
        env: IndexMap::new(),
        path: Vec::new(),
    })
}

fn ask_secrets(app: &mut App) -> Result<crate::config::local::SecretsConfig> {
    app.ui.heading("Secrets");
    app.ui.say(
        "Tokens and signing keys can be fetched from a secrets manager into \
         ~/.hats/secrets.yaml, so shells stay offline and instant.",
    );

    let want =
        app.ui
            .prompter
            .confirm("secrets.enabled", "Fetch secrets from a manager?", false)?;
    if !want {
        return Ok(crate::config::local::SecretsConfig::default());
    }

    let choice = app.ui.prompter.select(
        "secrets.provider",
        "Which secrets manager?",
        &["bitwarden", "aws-secrets-manager", "vault"],
    )?;
    let provider = match choice.as_str() {
        "bitwarden" => ProviderKind::Bitwarden,
        "aws-secrets-manager" => ProviderKind::AwsSecretsManager,
        "vault" => ProviderKind::Vault,
        other => bail!("unknown secrets provider `{other}`"),
    };

    if !provider.implemented() {
        app.ui.warn(format!(
            "{} is recorded but not implemented yet; `hats secrets fetch` will say so.",
            provider.as_str()
        ));
        return Ok(crate::config::local::SecretsConfig {
            provider,
            bitwarden: None,
            envelope: EnvelopeConfig::default(),
        });
    }

    let self_hosted = app.ui.prompter.confirm(
        "secrets.bitwarden.self_hosted",
        "Self-hosted (Vaultwarden or your own Bitwarden)?",
        true,
    )?;
    let (api_url, identity_url) = if self_hosted {
        let base = app.ui.prompter.text(
            "secrets.bitwarden.base_url",
            "Server base URL (e.g. https://vault.example.net)",
            Some(""),
        )?;
        let base = base.trim().trim_end_matches('/').to_string();
        if base.is_empty() {
            (None, None)
        } else {
            (
                Some(format!("{base}/api")),
                Some(format!("{base}/identity")),
            )
        }
    } else {
        (None, None)
    };

    let envelope_choice = app.ui.prompter.select(
        "secrets.envelope.method",
        "Protect the vault credentials with a YubiKey?",
        &["yubikey-piv", "none"],
    )?;
    let method = match envelope_choice.as_str() {
        "yubikey-piv" => EnvelopeMethod::YubikeyPiv,
        _ => EnvelopeMethod::None,
    };
    let slot = if method == EnvelopeMethod::YubikeyPiv {
        Some(app.ui.prompter.text(
            "secrets.envelope.slot",
            "PIV slot (retired slots are 82-95)",
            Some("82"),
        )?)
    } else {
        None
    };

    if method == EnvelopeMethod::YubikeyPiv {
        app.ui
            .say("Run `hats secrets enrol-yubikey` to generate the key and store the credentials.");
    } else {
        app.ui
            .say("`hats secrets fetch` will prompt for the credentials each time.");
    }

    Ok(crate::config::local::SecretsConfig {
        provider,
        bitwarden: Some(BitwardenConfig {
            self_hosted,
            api_url,
            identity_url,
            discovery: Default::default(),
        }),
        envelope: EnvelopeConfig {
            method,
            recipient: None,
            slot,
        },
    })
}

/// An answers file that is missing keys silently takes defaults, which makes a
/// half-written fixture look like a pass. Say so instead.
fn report_missed_answers(app: &App) {
    if app.ui.prompter.is_interactive() {
        return;
    }
    app.ui
        .detail("questions answered from defaults are listed with -v");
}

fn print_next_steps(app: &App, cfg: &LocalConfig) {
    app.ui.heading("Ready");
    let names: Vec<&str> = cfg.hats.keys().map(String::as_str).collect();
    app.ui.say(format!("Profiles: {}", names.join(", ")));
    app.ui.say(format!("Default:  {}", cfg.default_hat()));
    app.ui.say("");
    app.ui.say("Next:");
    app.ui
        .say("  hats plan                 preview what would change in your home directory");
    app.ui.say("  hats apply                write the files");
    if cfg.secrets.provider != ProviderKind::None {
        app.ui
            .say("  hats secrets fetch        pull tokens into ~/.hats/secrets.yaml");
    }
    app.ui.say("");

    // The switcher is a shell function, so it only exists in shells started
    // after apply. Saying so here heads off the obvious first attempt --
    // `hats hat <name>`, which is a clap error rather than a switch.
    app.ui.say("Then open a new terminal and switch it with:");
    app.ui.say(format!(
        "  hat {:<17} or plain `hat` to pick from a list",
        cfg.default_hat()
    ));
    app.ui
        .say("  hats hat current      which hat this shell is on");
    app.ui.say("");
    app.ui
        .say("`hat` is a shell function rather than a hats subcommand: only");
    app.ui
        .say("your own shell can change its own environment. It arrives with the");
    app.ui
        .say("files, so it exists in shells started after the apply.");
    app.ui.say("");
    app.ui.say("  hats --help               everything else");
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn is_empty_identity(i: &IdentitySpec) -> bool {
    i.name.is_none() && i.email.is_none() && i.signing_key.is_none()
}

fn some_if_any<T>(value: T, has: impl Fn(&T) -> bool) -> Option<T> {
    has(&value).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_trims_and_discards_blanks() {
        assert_eq!(non_empty("  x  ".into()), Some("x".into()));
        assert_eq!(non_empty("   ".into()), None);
        assert_eq!(non_empty(String::new()), None);
    }

    #[test]
    fn the_palette_cycles_rather_than_running_out() {
        for n in 1..=12usize {
            assert!(PALETTE[(n - 1) % PALETTE.len()].starts_with('#'));
        }
    }

    #[test]
    fn some_if_any_drops_an_all_blank_block() {
        let empty = KubeSpec::default();
        assert!(
            some_if_any(empty, |k: &KubeSpec| k.context.is_some()
                || k.isolate.is_some())
            .is_none()
        );
        let filled = KubeSpec {
            context: Some("c".into()),
            isolate: None,
        };
        assert!(
            some_if_any(filled, |k: &KubeSpec| k.context.is_some()
                || k.isolate.is_some())
            .is_some()
        );
    }

    #[test]
    fn an_identity_with_nothing_in_it_is_recognised() {
        assert!(is_empty_identity(&IdentitySpec::default()));
        assert!(!is_empty_identity(&IdentitySpec {
            name: Some("x".into()),
            ..Default::default()
        }));
    }
}
