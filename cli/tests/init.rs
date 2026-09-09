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

  profile.1.name: normal
  profile.1.aws_profile: default
  profile.1.aws_region: eu-west-2
  profile.1.kube_context: ""
  profile.1.colour: "#2a2040"

  profile.add.2: true
  profile.2.name: acme
  profile.2.inherits: true
  profile.2.git_name: Jane Doe
  profile.2.git_email: jane.doe@acme.example
  profile.2.aws_profile: acme-aws
  profile.2.aws_region: eu-west-2
  profile.2.kube_context: acme
  profile.2.colour: "#331420"

  profile.add.3: false

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
    repo: PathBuf,
    answers: PathBuf,
}

impl Env {
    fn new(tag: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hats-home");
        let repo = fixture_repo(dir.path(), tag);
        let answers = dir.path().join("answers.yaml");
        std::fs::write(&answers, ANSWERS).unwrap();
        Self {
            _dir: dir,
            home,
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
            // Never inherit the developer's own profile state into a test.
            .env_remove("HATS_PROFILE")
            .env_remove("DEV_PROFILE")
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

    // Both profiles, with inheritance and the per-profile overrides.
    assert!(cfg.contains("normal:"));
    assert!(cfg.contains("acme:"));
    assert!(cfg.contains("inherits: normal"));
    assert!(cfg.contains("acme-aws"));
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
fn the_first_profile_becomes_the_default() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    assert!(env.config_text().contains("default_profile: normal"));
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
fn profile_list_shows_what_was_configured() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    env.hats()
        .args(["profile", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("normal").and(predicate::str::contains("acme")));

    // --plain is what fzf and shell completion consume: names only.
    let out = env
        .hats()
        .args(["profile", "list", "--plain"])
        .output()
        .unwrap();
    let names: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();
    assert_eq!(names, vec!["normal", "acme"]);
}

#[test]
fn profile_show_folds_the_inheritance_chain() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    env.hats()
        .args(["profile", "show", "acme"])
        .assert()
        .success()
        // Overridden on the child.
        .stdout(predicate::str::contains("jane.doe@acme.example"))
        .stdout(predicate::str::contains("acme-aws"))
        // Inherited from normal.
        .stdout(predicate::str::contains("eu-west-2"));
}

#[test]
fn profile_show_names_a_profile_that_does_not_exist() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.hats()
        .args(["profile", "show", "ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ghost"))
        .stderr(predicate::str::contains("normal"));
}

/// The regression guard for the original `_profile_reset` leak: a variable set
/// by one profile must be unset when switching to a profile that does not set
/// it.
#[test]
fn the_reset_list_covers_variables_from_every_profile() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    let out = env.hats().args(["profile", "reset-list"]).output().unwrap();
    let keys: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();

    for expected in [
        "AWS_PROFILE",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "HATS_PROFILE",
        "DEV_PROFILE",
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
        .stdout(predicate::str::contains("2 profiles"))
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
        .args(["profile", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("brew upgrade hats"));
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
