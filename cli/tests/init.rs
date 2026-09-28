//! End-to-end coverage of `hats init` and the commands that read what it
//! writes.
//!
//! The wizard runs unattended here through `--answers`, which is the point of
//! routing every question through one `Prompter`: the interactive and the
//! recorded paths are the same code. There is no network anywhere: hats has
//! no repo to clone since the dotfiles engine moved out to bosun.

use std::path::{Path, PathBuf};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

const ANSWERS: &str = r##"
answers:
  identity.name: Jane
  identity.email: jane@example.com

  features.ssh: true
  features.vscode: false

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
    answers: PathBuf,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hats-home");
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(&user_home).unwrap();
        let answers = dir.path().join("answers.yaml");
        std::fs::write(&answers, ANSWERS).unwrap();
        Self {
            _dir: dir,
            home,
            user_home,
            answers,
        }
    }

    fn hats(&self) -> Command {
        let mut c = Command::cargo_bin("hats").unwrap();
        c.arg("--hats-home")
            .arg(&self.home)
            .arg("--no-color")
            // Per-hat files land in a throwaway home, never the developer's.
            .env("HOME", &self.user_home)
            // Never inherit the developer's own hat state into a test.
            .env_remove("HATS_HAT")
            .env_remove("HATS_HAT_FILE")
            .env_remove("HATS_HAT_PREV")
            .env_remove("HATS_HOME");
        c
    }

    fn init(&self) -> Command {
        let mut c = self.hats();
        c.arg("--answers").arg(&self.answers).arg("init");
        c
    }

    fn config_text(&self) -> String {
        std::fs::read_to_string(self.home.join("config.yaml")).unwrap()
    }
}

#[test]
fn init_writes_a_config_and_scaffolds_without_any_network() {
    let env = Env::new();
    env.init().assert().success();

    assert!(
        env.home.join("config.yaml").is_file(),
        "config was not written"
    );
    assert!(env.home.join("backups").is_dir());

    let cfg = env.config_text();
    assert!(
        cfg.starts_with("# ~/.hats/config.yaml"),
        "header missing:\n{cfg}"
    );

    // init runs a sync, so the base skeletons and every hat's files exist.
    for rel in [
        ".gitconfig",
        ".ssh/config",
        ".ssh/config.d/README",
        ".ssh/config.d/common.conf",
        ".ssh/config.d/normal.conf",
        ".ssh/config.d/acme.conf",
        ".terraformrc",
        ".tofurc",
        ".gitconfig.d/normal",
        ".gitconfig.d/acme",
        ".kube/config.acme",
    ] {
        assert!(
            env.user_home.join(rel).is_file(),
            "{rel} was not scaffolded"
        );
    }
    assert!(env.user_home.join(".ssh/known_hosts.d").is_dir());
    assert!(env.user_home.join(".cache/opentofu/plugin-cache").is_dir());

    let gitconfig = std::fs::read_to_string(env.user_home.join(".gitconfig")).unwrap();
    assert!(
        gitconfig.contains("email = jane@example.com"),
        "{gitconfig}"
    );
    assert!(
        gitconfig.contains("path = ~/.config/git/style.gitconfig"),
        "{gitconfig}"
    );
}

#[test]
fn the_wizard_records_every_answer_it_was_given() {
    let env = Env::new();
    env.init().assert().success();
    let cfg = env.config_text();

    // Identity and both hats, with inheritance and the per-hat overrides.
    assert!(cfg.contains("jane@example.com"));
    assert!(cfg.contains("normal:"));
    assert!(cfg.contains("acme:"));
    assert!(cfg.contains("inherits: normal"));
    assert!(cfg.contains("context: acme"));
    assert!(cfg.contains("jane.doe@acme.example"));

    // Secrets: provider and endpoints derived from the base URL, plus the
    // expected key that only the gitconfig scaffold reads.
    assert!(cfg.contains("provider: bitwarden"));
    assert!(cfg.contains("https://vault.example.net/api"));
    assert!(cfg.contains("https://vault.example.net/identity"));
    assert!(cfg.contains("method: yubikey-piv"));
    assert!(cfg.contains("git_signing_key"));

    // No credential ever reaches the config file.
    assert!(
        !cfg.contains("password"),
        "config must not hold credentials:\n{cfg}"
    );
}

#[test]
fn the_first_hat_becomes_the_default() {
    let env = Env::new();
    env.init().assert().success();
    assert!(env.config_text().contains("default_hat: normal"));
}

#[test]
fn init_refuses_to_clobber_an_existing_setup_without_force() {
    let env = Env::new();
    env.init().assert().success();
    env.init()
        .assert()
        .failure()
        .stderr(predicate::str::contains("already set up"));
    env.init().arg("--force").assert().success();
}

#[test]
fn hat_list_shows_what_was_configured() {
    let env = Env::new();
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
    let env = Env::new();
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
    let env = Env::new();
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
    let env = Env::new();
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

/// A second sync is a no-op, and one after a hand-deleted scaffold recreates
/// only what is missing.
#[test]
fn hat_sync_settles_and_recreates_what_is_missing() {
    let env = Env::new();
    env.init().assert().success();

    env.hats()
        .args(["hat", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Everything is already in place"));

    // Edits survive a sync: the scaffolds are created once, never updated.
    let conf = env.user_home.join(".ssh/config.d/acme.conf");
    std::fs::write(&conf, "Host mine\n").unwrap();
    std::fs::remove_file(env.user_home.join(".gitconfig.d/acme")).unwrap();

    env.hats()
        .args(["hat", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains(".gitconfig.d/acme"));
    assert_eq!(std::fs::read_to_string(&conf).unwrap(), "Host mine\n");
}

/// `hats env --here` is what the shell's `cd` hook evals. Walk one shell into
/// a tree that names a hat, around inside it, and back out.
#[test]
fn env_here_follows_the_nearest_dot_hat_file() {
    let env = Env::new();
    env.init().assert().success();

    // Canonical, as the binary's own `current_dir` reports it.
    std::fs::create_dir_all(env.user_home.join("work")).unwrap();
    let tree = env.user_home.join("work").canonicalize().unwrap();
    std::fs::write(tree.join(".hat"), "# this tree is acme's\nacme\n").unwrap();
    let sub = tree.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let file = tree.join(".hat");
    let here = |dir: &Path, vars: &[(&str, &str)]| {
        let mut c = env.hats();
        c.args(["env", "--here", "--quiet"]).current_dir(dir);
        for (k, v) in vars {
            c.env(k, v);
        }
        let out = c.output().unwrap();
        assert!(out.status.success());
        (
            String::from_utf8(out.stdout).unwrap(),
            String::from_utf8(out.stderr).unwrap(),
        )
    };
    let file_str = file.to_str().unwrap();

    // Outside every tree there is nothing to say.
    let (script, _) = here(&env.user_home, &[("HATS_HAT", "normal")]);
    assert_eq!(script, "");

    // Entering: the folder's hat goes on, and the old one is remembered.
    let (script, _) = here(&sub, &[("HATS_HAT", "normal")]);
    assert!(script.contains("export HATS_HAT=acme"), "{script}");
    assert!(script.contains("export HATS_HAT_PREV=normal"), "{script}");
    assert!(script.contains("HATS_HAT_FILE="), "{script}");
    assert!(script.contains(file_str), "{script}");

    // Moving within the tree says nothing, even after `hat normal` by hand.
    let inside = [
        ("HATS_HAT", "normal"),
        ("HATS_HAT_FILE", file_str),
        ("HATS_HAT_PREV", "normal"),
    ];
    let (script, _) = here(&tree, &inside);
    assert_eq!(script, "", "a manual choice sticks inside the tree");

    // Leaving with the previous hat already on only clears the folder state.
    let (script, _) = here(&env.user_home, &inside);
    assert_eq!(script, "unset HATS_HAT_FILE HATS_HAT_PREV 2>/dev/null\n");

    // Leaving while wearing the folder's hat switches back.
    let wearing = [
        ("HATS_HAT", "acme"),
        ("HATS_HAT_FILE", file_str),
        ("HATS_HAT_PREV", "normal"),
    ];
    let (script, _) = here(&env.user_home, &wearing);
    assert!(script.contains("export HATS_HAT=normal"), "{script}");
    let unset = script.lines().find(|l| l.starts_with("unset ")).unwrap();
    assert!(unset.contains("HATS_HAT_FILE") && unset.contains("HATS_HAT_PREV"));

    // A folder that asks for the hat already on moves only the folder state.
    let (script, _) = here(&sub, &[("HATS_HAT", "acme")]);
    assert!(!script.contains("export HATS_HAT=acme"), "{script}");
    assert!(script.contains("export HATS_HAT_PREV=acme"), "{script}");

    // A file naming a hat this machine lacks switches nothing, and says why.
    std::fs::write(&file, "globex\n").unwrap();
    let (script, warning) = here(&sub, &[("HATS_HAT", "normal")]);
    assert_eq!(script, "", "a broken .hat must never half-apply");
    assert!(warning.contains("globex"), "{warning}");
}

/// The same walk, through a real zsh running the real integration script: the
/// `chpwd` hook is the half of the feature the binary cannot test on its own.
#[test]
fn the_zsh_cd_hook_switches_hats_as_the_shell_moves() {
    let Ok(zsh) = which::which("zsh") else {
        return;
    };
    let env = Env::new();
    env.init().assert().success();

    std::fs::create_dir_all(env.user_home.join("work/sub")).unwrap();
    std::fs::write(env.user_home.join("work/.hat"), "acme\n").unwrap();

    let bin = assert_cmd::cargo::cargo_bin("hats");
    let path = format!(
        "{}:{}",
        bin.parent().unwrap().display(),
        std::env::var("PATH").unwrap()
    );
    let script = r#"
        eval "$(hats shell-init zsh)"
        echo "start=$HATS_HAT"
        cd ~/work/sub;  echo "entered=$HATS_HAT prev=$HATS_HAT_PREV"
        hat normal >/dev/null
        cd ~/work;      echo "manual=$HATS_HAT"
        cd ~;           echo "left=$HATS_HAT file=${HATS_HAT_FILE:-none}"
        cd ~/work;      echo "again=$HATS_HAT"
        cd ~;           echo "back=$HATS_HAT"
    "#;
    let out = std::process::Command::new(zsh)
        .args(["-f", "-c", script])
        .current_dir(&env.user_home)
        .env("PATH", path)
        .env("HOME", &env.user_home)
        .env("HATS_HOME", &env.home)
        .env("NO_COLOR", "1")
        .env_remove("HATS_HAT")
        .env_remove("HATS_HAT_FILE")
        .env_remove("HATS_HAT_PREV")
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();

    for expected in [
        "start=normal",
        "entered=acme prev=normal",
        "manual=normal",
        "left=normal file=none",
        "again=acme",
        "back=normal",
    ] {
        assert!(
            stdout.lines().any(|l| l == expected),
            "missing `{expected}`\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
    // A hat that changed under a `cd` is announced.
    assert!(stdout.contains("hat: acme"), "{stdout}");
}

#[test]
fn doctor_passes_on_a_freshly_initialised_machine() {
    let env = Env::new();
    env.init().assert().success();
    env.hats()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 hats"))
        .stdout(predicate::str::contains("resolve cleanly"))
        .stdout(predicate::str::contains("every per-hat file exists"));
}

#[test]
fn doctor_before_init_fails_and_says_to_run_init() {
    let env = Env::new();
    env.hats()
        .arg("doctor")
        .assert()
        .failure()
        .stdout(predicate::str::contains("hats init"));
}

#[test]
fn version_reports_the_binary() {
    let env = Env::new();
    env.hats()
        .args(["version", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(concat!(
            "\"version\":\"",
            env!("CARGO_PKG_VERSION")
        )));
}

#[test]
fn hat_create_adds_the_hat_and_makes_its_files() {
    let env = Env::new();
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
        ".ssh/config.d/globex.conf",
        ".aws/.hats/globex.config",
        ".aws/.hats/globex.credentials",
        ".kube/config.globex",
    ] {
        assert!(env.user_home.join(rel).is_file(), "{rel} was not created");
    }
    assert!(env.user_home.join(".config/coderv2/hats/globex").is_dir());

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
    let env = Env::new();
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
    let env = Env::new();
    env.init().assert().success();
    env.hats()
        .args(["--non-interactive", "hat", "create", "globex"])
        .assert()
        .success();

    // A file hats never manages: a k9s plugin the user added to the hat's own
    // directory.
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
    let env = Env::new();
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
    let env = Env::new();
    env.hats()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_hats"));
}
