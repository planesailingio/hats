//! End-to-end coverage of `hats init` and the commands that read what it
//! writes.
//!
//! The wizard runs unattended here through `--answers`, which is the point of
//! routing every question through one `Prompter`: the interactive and the
//! recorded paths are the same code.

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

/// A throwaway dotfiles repo with a manifest, tagged so version checks have
/// something real to compare against.
fn fixture_repo(dir: &Path, tag: Option<&str>) -> PathBuf {
    let repo = dir.join("dotfiles");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("hats.yaml"),
        r#"
hats:
  schema: 1
groups:
  shell: { description: "zsh config", default: true }
  theme: { description: "colours", default: true }
  extras: { description: "optional bits", default: false }
files:
  - { path: .zshrc.j2, group: shell }
  - { path: .config/starship.toml, group: theme }
  - { path: .env.d, group: extras }
secrets:
  required: [git_signing_key]
"#,
    )
    .unwrap();

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "--quiet", "-b", "main"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "T"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "manifest"]);
    if let Some(t) = tag {
        git(&["tag", "-a", t, "-m", t]);
    }
    repo
}

const ANSWERS: &str = r##"
answers:
  identity.name: Jane
  identity.email: jane@example.com

  groups.shell: true
  groups.theme: true
  groups.extras: false

  hat.1.name: normal
  hat.1.kube_context: ""
  hat.1.colour: "#2a2040"

  hat.add.2: true
  hat.2.name: acme
  hat.2.inherits: true
  hat.2.git_name: Jane Doe
  hat.2.git_email: jane.doe@acme.example
  hat.2.kube_context: acme
  hat.2.colour: "#331420"

  hat.add.3: false

  secrets.enabled: true
  secrets.provider: bitwarden
  secrets.bitwarden.self_hosted: true
  secrets.bitwarden.base_url: https://vault.example.net
  secrets.envelope.method: yubikey-piv
  secrets.envelope.slot: "82"
"##;

struct Env {
    _dir: tempfile::TempDir,
    home: PathBuf,
    /// Stands in for `$HOME`, where the per-hat files go.
    user_home: PathBuf,
    repo: PathBuf,
    answers: PathBuf,
}

impl Env {
    fn new(tag: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hats-home");
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(&user_home).unwrap();
        let repo = fixture_repo(dir.path(), tag);
        let answers = dir.path().join("answers.yaml");
        std::fs::write(&answers, ANSWERS).unwrap();
        Self {
            _dir: dir,
            home,
            user_home,
            repo,
            answers,
        }
    }

    fn hats(&self) -> Command {
        let mut c = Command::cargo_bin("hats").unwrap();
        c.arg("--hats-home")
            .arg(&self.home)
            .arg("--no-color")
            .arg("--allow-mismatch")
            // Per-hat files land in a throwaway home, never the developer's.
            .env("HOME", &self.user_home)
            // Never inherit the developer's own hat state into a test.
            .env_remove("HATS_HAT")
            .env_remove("HATS_HOME")
            .env_remove("HATS_DEV");
        c
    }

    fn init(&self) -> Command {
        let mut c = self.hats();
        c.arg("--answers")
            .arg(&self.answers)
            .arg("init")
            .arg("--repo")
            .arg(&self.repo);
        c
    }

    fn config_text(&self) -> String {
        std::fs::read_to_string(self.home.join("config.yaml")).unwrap()
    }
}

#[test]
fn init_clones_the_repo_and_writes_a_config() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    assert!(
        env.home.join("repo/hats.yaml").is_file(),
        "repo was not cloned"
    );
    assert!(
        env.home.join("config.yaml").is_file(),
        "config was not written"
    );
    assert!(env.home.join("backups").is_dir());
    assert!(env.home.join("plans").is_dir());

    let cfg = env.config_text();
    assert!(
        cfg.starts_with("# ~/.hats/config.yaml"),
        "header missing:\n{cfg}"
    );
}

#[test]
fn the_wizard_records_every_answer_it_was_given() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    let cfg = env.config_text();

    // Identity, groups, and the group the answers turned off.
    assert!(cfg.contains("jane@example.com"));
    assert!(cfg.contains("shell: true"));
    assert!(cfg.contains("extras: false"));

    // Both hats, with inheritance and the per-hat overrides.
    assert!(cfg.contains("normal:"));
    assert!(cfg.contains("acme:"));
    assert!(cfg.contains("inherits: normal"));
    assert!(cfg.contains("context: acme"));
    assert!(cfg.contains("jane.doe@acme.example"));

    // Secrets: provider and endpoints derived from the base URL.
    assert!(cfg.contains("provider: bitwarden"));
    assert!(cfg.contains("https://vault.example.net/api"));
    assert!(cfg.contains("https://vault.example.net/identity"));
    assert!(cfg.contains("method: yubikey-piv"));

    // No credential ever reaches the config file.
    assert!(
        !cfg.contains("password"),
        "config must not hold credentials:\n{cfg}"
    );
}

#[test]
fn the_first_hat_becomes_the_default() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    assert!(env.config_text().contains("default_hat: normal"));
}

#[test]
fn init_refuses_to_clobber_an_existing_setup_without_force() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.init()
        .assert()
        .failure()
        .stderr(predicate::str::contains("already set up"));
    env.init().arg("--force").assert().success();
}

#[test]
fn hat_list_shows_what_was_configured() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    env.hats()
        .args(["hat", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("normal").and(predicate::str::contains("acme")));

    // --plain is what fzf and shell completion consume: names only.
    let out = env
        .hats()
        .args(["hat", "list", "--plain"])
        .output()
        .unwrap();
    let names: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();
    assert_eq!(names, vec!["normal", "acme"]);
}

#[test]
fn hat_show_folds_the_inheritance_chain() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    env.hats()
        .args(["hat", "show", "acme"])
        .assert()
        .success()
        // Overridden on the child.
        .stdout(predicate::str::contains("jane.doe@acme.example"))
        .stdout(predicate::str::contains("kube      acme"))
        // Folded in from the parent rather than restated on the child.
        .stdout(predicate::str::contains("inherits  normal"))
        .stdout(predicate::str::contains("aws       isolated: true"));
}

#[test]
fn hat_show_names_a_hat_that_does_not_exist() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.hats()
        .args(["hat", "show", "ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ghost"))
        .stderr(predicate::str::contains("normal"));
}

/// The regression guard for the original `_profile_reset` leak: a variable set
/// by one hat must be unset when switching to a hat that does not set
/// it.
#[test]
fn the_reset_list_covers_variables_from_every_hat() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    let out = env.hats().args(["hat", "reset-list"]).output().unwrap();
    let keys: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();

    for expected in [
        "AWS_PROFILE",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "HATS_HAT",
        "KUBECONFIG",
    ] {
        assert!(
            keys.contains(&expected),
            "reset list is missing {expected}: {keys:?}"
        );
    }
}

#[test]
fn doctor_passes_on_a_freshly_initialised_machine() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.hats()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 hats"))
        .stdout(predicate::str::contains("resolve cleanly"));
}

#[test]
fn doctor_before_init_fails_and_says_to_run_init() {
    let env = Env::new(Some("v0.1.0"));
    env.hats()
        .arg("doctor")
        .assert()
        .failure()
        .stdout(predicate::str::contains("hats init"));
}

#[test]
fn version_reports_the_binary_and_the_repo() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.hats()
        .args(["version", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(concat!(
            "\"version\":\"",
            env!("CARGO_PKG_VERSION")
        )))
        .stdout(predicate::str::contains("\"repo_tag\""));
}

/// `--check` is the machine-readable half of the lockstep: 0 in step, 3 repo
/// behind, 4 binary behind.
#[test]
fn update_check_reports_a_repo_behind_the_binary() {
    // Tagged with something older than this binary will ever be.
    let env = Env::new(Some("v0.0.1"));
    env.init().assert().success();
    env.hats()
        .args(["update", "--check"])
        .assert()
        .code(3)
        .stdout(predicate::str::contains("hats update"));
}

#[test]
fn update_check_reports_a_repo_ahead_of_the_binary() {
    let env = Env::new(Some("v999.0.0"));
    env.init().assert().success();
    env.hats()
        .args(["update", "--check"])
        .assert()
        .code(4)
        .stdout(predicate::str::contains("brew upgrade"));
}

#[test]
fn update_check_is_clean_when_the_tags_agree() {
    let env = Env::new(Some(concat!("v", env!("CARGO_PKG_VERSION"))));
    env.init().assert().success();
    env.hats().args(["update", "--check"]).assert().code(0);
}

#[test]
fn update_explains_a_tag_that_has_not_been_published_yet() {
    let env = Env::new(Some("v0.0.1"));
    env.init().assert().success();
    env.hats()
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no tag"));
}

#[test]
fn a_repo_with_a_newer_schema_is_refused_with_an_upgrade_hint() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    std::fs::write(env.home.join("repo/hats.yaml"), "hats:\n  schema: 99\n").unwrap();
    env.hats()
        .args(["hat", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("brew upgrade hats"));
}

#[test]
fn hat_create_adds_the_hat_and_makes_its_files() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    env.hats()
        .args(["--non-interactive", "hat", "create", "globex"])
        .args([
            "--git-email",
            "jane@globex.example",
            "--kube-context",
            "globex-prod",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("added hat `globex`"))
        .stdout(predicate::str::contains("hat globex"));

    // Inherits the default hat and records only the difference.
    let cfg = env.config_text();
    assert!(cfg.contains("globex:"), "{cfg}");
    assert!(cfg.contains("inherits: normal"), "{cfg}");
    assert!(cfg.contains("jane@globex.example"), "{cfg}");
    assert!(cfg.contains("context: globex-prod"), "{cfg}");
    let globex = &cfg[cfg.find("  globex:").unwrap()..];
    let globex = &globex[..globex.find("\nsecrets:").unwrap_or(globex.len())];
    assert!(
        !globex.contains("name:"),
        "the inherited git name must not be restated:\n{globex}"
    );

    for rel in [
        ".gitconfig.d/globex",
        ".aws/.hats/globex.config",
        ".aws/.hats/globex.credentials",
        ".kube/config.globex",
    ] {
        assert!(env.user_home.join(rel).is_file(), "{rel} was not created");
    }
    assert!(env.user_home.join(".config/coderv2/hats/globex").is_dir());
    // The fixture repo manages no ~/.ssh/config, so nothing would read one.
    assert!(!env.user_home.join(".ssh/config.d/globex.conf").exists());
    // Only the new hat's files: the others wait for an apply.
    assert!(!env.user_home.join(".gitconfig.d/acme").exists());

    let out = env
        .hats()
        .args(["hat", "list", "--plain"])
        .output()
        .unwrap();
    let names: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();
    assert_eq!(names, vec!["normal", "acme", "globex"]);
}

#[test]
fn hat_create_refuses_a_taken_unsafe_or_reserved_name() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    let before = env.config_text();

    for (name, why) in [
        ("acme", "already exists"),
        ("../evil", "not a usable hat name"),
        ("common", "reserved"),
    ] {
        env.hats()
            .args(["--non-interactive", "hat", "create", name])
            .assert()
            .failure()
            .stderr(predicate::str::contains(why));
    }
    env.hats()
        .args([
            "--non-interactive",
            "hat",
            "create",
            "orphan",
            "--inherits",
            "ghost",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ghost"));

    assert_eq!(env.config_text(), before, "a refused create writes nothing");
}

#[test]
fn hat_delete_removes_the_hat_and_moves_every_file_to_the_backup() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.hats()
        .args(["--non-interactive", "hat", "create", "globex"])
        .assert()
        .success();

    // Files hats never manages: an ssh block written by hand, and a k9s
    // plugin the user added to the hat's own directory.
    let ssh = env.user_home.join(".ssh/config.d/globex.conf");
    std::fs::create_dir_all(ssh.parent().unwrap()).unwrap();
    std::fs::write(&ssh, "Host github.com\n  IdentityFile ~/.ssh/id_globex\n").unwrap();
    let plugin = env.user_home.join(".config/k9s/hats/globex/plugins.yaml");
    std::fs::create_dir_all(plugin.parent().unwrap()).unwrap();
    std::fs::write(&plugin, "plugins: {}\n").unwrap();

    env.hats()
        .args(["hat", "delete", "globex", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed hat `globex`"));

    assert!(!env.config_text().contains("globex"));
    for rel in [
        ".ssh/config.d/globex.conf",
        ".gitconfig.d/globex",
        ".aws/.hats/globex.config",
        ".aws/.hats/globex.credentials",
        ".kube/config.globex",
        ".config/k9s/hats/globex",
        ".config/coderv2/hats/globex",
    ] {
        assert!(
            env.user_home.join(rel).symlink_metadata().is_err(),
            "{rel} survived the delete"
        );
    }

    // Nothing destroyed outright: the files and the old config are in one
    // backup directory.
    let backups: Vec<PathBuf> = std::fs::read_dir(env.home.join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let kept = |rel: &str| backups.iter().any(|b| b.join(rel).exists());
    assert!(kept(".ssh/config.d/globex.conf"));
    assert!(kept(".config/k9s/hats/globex/plugins.yaml"));
    assert!(kept(".kube/config.globex"));
    assert!(kept("config.yaml"));

    // The rest of the machine is untouched.
    env.hats().args(["hat", "show", "acme"]).assert().success();
}

#[test]
fn hat_delete_asks_first_and_refuses_what_would_break_the_config() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    let before = env.config_text();

    // No --yes and nobody to ask: the default answer is no.
    env.hats()
        .args(["--non-interactive", "hat", "delete", "acme"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Nothing deleted"));

    // acme inherits from normal, which is also the default hat.
    env.hats()
        .args(["hat", "delete", "normal", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("acme inherits from `normal`"));

    env.hats()
        .args(["hat", "delete", "ghost", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no hat named `ghost`"));

    assert_eq!(env.config_text(), before, "a refused delete writes nothing");
}

#[test]
fn completions_are_generated_for_zsh() {
    let env = Env::new(None);
    env.hats()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_hats"));
}
