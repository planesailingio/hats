//! Carrying out a plan.
//!
//! Three properties matter more than speed:
//!
//! * **Nothing is lost.** Every file about to be overwritten or pruned is
//!   copied into `~/.hats/backups/<timestamp>/` first.
//! * **No half-written file.** Content goes to a temp file in the destination
//!   directory and is renamed into place, so a crash leaves either the old file
//!   or the new one.
//! * **A failure is resumable.** State is written for everything that
//!   succeeded, so re-running continues rather than starting over.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::repo::Phase;
use crate::engine::plan::{Action, Entry, Plan};
use crate::engine::state::State;
use crate::hat::scaffold;
use crate::hooks;
use crate::platform::Platform;

/// What an apply is allowed to do.
#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    pub only_files: bool,
    pub hooks_only: bool,
    /// Leave orphaned files alone.
    pub no_prune: bool,
    /// Prune even files the user has edited since hats wrote them.
    pub force_prune: bool,
}

/// What an apply did, for the closing report.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub created: usize,
    pub updated: usize,
    pub mode_changed: usize,
    pub destroyed: usize,
    pub scaffolded: usize,
    pub skipped: Vec<String>,
    pub hooks_run: Vec<String>,
    pub backup_dir: Option<PathBuf>,
}

impl Outcome {
    pub fn line(&self) -> String {
        format!(
            "Apply complete: {} added, {} changed, {} destroyed, {} permission change{}, {} scaffolded; {} hook{} run.",
            self.created,
            self.updated,
            self.destroyed,
            self.mode_changed,
            if self.mode_changed == 1 { "" } else { "s" },
            self.scaffolded,
            self.hooks_run.len(),
            if self.hooks_run.len() == 1 { "" } else { "s" },
        )
    }
}

/// Everything apply needs that is not in the plan.
pub struct ApplyContext<'a> {
    pub platform: &'a Platform,
    pub repo_dir: PathBuf,
    pub files_dir: PathBuf,
    pub hats_home: PathBuf,
    pub backups_root: PathBuf,
    pub state_path: PathBuf,
}

/// Apply `plan`, mutating and persisting `state` as it goes.
pub fn apply(
    plan: &Plan,
    state: &mut State,
    ctx: &ApplyContext<'_>,
    opts: &ApplyOptions,
    mut report: impl FnMut(&str),
) -> Result<Outcome> {
    let mut outcome = Outcome::default();
    // One directory per apply, so a rollback is a single copy back.
    let backup = ctx
        .backups_root
        .join(chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string());

    let hook_env = hooks::environment(ctx.platform, &ctx.repo_dir, &ctx.files_dir, &ctx.hats_home);

    // `before` hooks run first: they exist to make the machine capable of
    // receiving the files (installing Homebrew, say).
    if !opts.only_files {
        for hook in plan.hooks.iter().filter(|h| h.phase == Phase::Before) {
            report(&format!("hook {} ({})", hook.name, hook.reason.describe()));
            hooks::run(hook, &ctx.repo_dir, &hook_env)?;
            state.record_hook(&hook.name, hook.hash.clone());
            outcome.hooks_run.push(hook.name.clone());
            state.save(&ctx.state_path)?;
        }
    }

    if !opts.hooks_only {
        for entry in &plan.entries {
            match &entry.action {
                Action::Unchanged => {}
                Action::Conflict { why } => {
                    outcome.skipped.push(format!("{} ({why})", entry.display));
                }
                Action::Destroy { safe } => {
                    if opts.no_prune {
                        outcome
                            .skipped
                            .push(format!("{} (--no-prune)", entry.display));
                        continue;
                    }
                    if !safe && !opts.force_prune {
                        outcome.skipped.push(format!(
                            "{} (modified locally; --force-prune to remove)",
                            entry.display
                        ));
                        continue;
                    }
                    // Pruned files are moved to the backup, never deleted
                    // outright: "destroy" should still be undoable.
                    back_up(&entry.target, &backup, &ctx.platform.home, &mut outcome)?;
                    std::fs::remove_file(&entry.target)
                        .with_context(|| format!("removing {}", entry.target.display()))?;
                    state.forget_file(&entry.target);
                    outcome.destroyed += 1;
                    report(&format!("destroyed {}", entry.display));
                }
                Action::Create | Action::Update | Action::ModeChange { .. } => {
                    let Some(contents) = &entry.contents else {
                        continue;
                    };
                    if entry.target.exists() {
                        back_up(&entry.target, &backup, &ctx.platform.home, &mut outcome)?;
                    }
                    write_file(entry, contents)?;
                    state.record_file(&entry.target, contents, Some(entry.mode));
                    match entry.action {
                        Action::Create => {
                            outcome.created += 1;
                            report(&format!("created {}", entry.display));
                        }
                        Action::Update => {
                            outcome.updated += 1;
                            report(&format!("updated {}", entry.display));
                        }
                        _ => {
                            outcome.mode_changed += 1;
                            report(&format!("chmod {}", entry.display));
                        }
                    }
                }
            }
        }
        state.touch();
        state.save(&ctx.state_path)?;

        // After the managed files, so the k9s links have something to point
        // at, and after the state save, so a failure here cannot lose the
        // record of what was written. Scaffolds are never recorded: hats does
        // not own them.
        for s in &plan.scaffolds {
            if scaffold::create(s, &ctx.platform.home)? {
                outcome.scaffolded += 1;
                report(&format!("scaffolded {}", s.display));
            }
        }
    }

    if !opts.only_files {
        for hook in plan.hooks.iter().filter(|h| h.phase == Phase::After) {
            report(&format!("hook {} ({})", hook.name, hook.reason.describe()));
            hooks::run(hook, &ctx.repo_dir, &hook_env)?;
            state.record_hook(&hook.name, hook.hash.clone());
            outcome.hooks_run.push(hook.name.clone());
            // Saved per hook, so a later failure does not re-run the ones that
            // already succeeded.
            state.save(&ctx.state_path)?;
        }
    }

    Ok(outcome)
}

/// Copy a file into the backup directory, mirroring its path under `$HOME`.
fn back_up(target: &Path, backup: &Path, home: &Path, outcome: &mut Outcome) -> Result<()> {
    let rel = target.strip_prefix(home).unwrap_or(target);
    let dest = backup.join(rel);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::copy(target, &dest)
        .with_context(|| format!("backing up {} to {}", target.display(), dest.display()))?;
    outcome.backup_dir = Some(backup.to_path_buf());
    Ok(())
}

/// Write atomically, with the right mode applied before the file is visible.
fn write_file(entry: &Entry, contents: &[u8]) -> Result<()> {
    let parent = entry
        .target
        .parent()
        .context("target has no parent directory")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    if let Some(mode) = entry.dir_mode {
        set_mode(parent, mode)?;
    }

    // The temp file must be on the same filesystem as the target for the
    // rename to be atomic, so it goes in the destination directory.
    let tmp = parent.join(format!(
        ".{}.hats-tmp",
        entry
            .target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into())
    ));
    std::fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
    set_mode(&tmp, entry.mode)?;
    std::fs::rename(&tmp, &entry.target)
        .with_context(|| format!("replacing {}", entry.target.display()))?;
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting {mode:04o} on {}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testkit::*;

    #[test]
    fn applying_a_fresh_plan_creates_every_file() {
        let t = Harness::new();
        let outcome = t.apply();
        assert_eq!(outcome.created, 2);
        assert_eq!(outcome.updated, 0);
        assert!(t.home.path().join(".zshrc").is_file());
        assert!(t.home.path().join(".ssh/config").is_file());
        assert!(outcome.line().contains("2 added"));
    }

    #[test]
    fn declared_modes_are_applied_to_files_and_their_directories() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let t = Harness::new();
            t.apply();
            let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o7777;
            assert_eq!(mode(t.home.path().join(".ssh/config")), 0o600);
            assert_eq!(mode(t.home.path().join(".ssh")), 0o700);
            assert_eq!(mode(t.home.path().join(".zshrc")), 0o644, "default mode");
        }
    }

    #[test]
    fn applying_twice_is_a_no_op() {
        let t = Harness::new();
        t.apply();
        let second = t.apply();
        assert_eq!(
            second,
            Outcome {
                hooks_run: second.hooks_run.clone(),
                ..Default::default()
            }
        );
        assert_eq!(second.created, 0);
        assert_eq!(second.updated, 0);
    }

    #[test]
    fn an_overwritten_file_is_backed_up_first() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".zshrc"), "precious\n").unwrap();

        let outcome = t.apply();
        assert_eq!(outcome.updated, 1);
        let backup = outcome.backup_dir.expect("a backup should have been made");
        assert_eq!(
            std::fs::read_to_string(backup.join(".zshrc")).unwrap(),
            "precious\n"
        );
    }

    #[test]
    fn a_destroyed_file_is_moved_to_the_backup_not_deleted() {
        let t = Harness::new();
        t.apply();
        t.set_group("ssh", false);

        let outcome = t.apply();
        assert_eq!(outcome.destroyed, 1);
        assert!(!t.home.path().join(".ssh/config").exists());
        let backup = outcome.backup_dir.unwrap();
        assert!(
            backup.join(".ssh/config").is_file(),
            "destroy must be recoverable"
        );
    }

    #[test]
    fn a_locally_modified_orphan_is_kept_unless_forced() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".ssh/config"), "mine\n").unwrap();
        t.set_group("ssh", false);

        let kept = t.apply();
        assert_eq!(kept.destroyed, 0);
        assert!(kept.skipped.iter().any(|s| s.contains("modified locally")));
        assert!(t.home.path().join(".ssh/config").exists());

        let forced = t.apply_with(ApplyOptions {
            force_prune: true,
            ..Default::default()
        });
        assert_eq!(forced.destroyed, 1);
        assert!(!t.home.path().join(".ssh/config").exists());
    }

    #[test]
    fn no_prune_leaves_orphans_entirely_alone() {
        let t = Harness::new();
        t.apply();
        t.set_group("ssh", false);

        let outcome = t.apply_with(ApplyOptions {
            no_prune: true,
            ..Default::default()
        });
        assert_eq!(outcome.destroyed, 0);
        assert!(t.home.path().join(".ssh/config").exists());
    }

    #[test]
    fn a_conflict_is_skipped_rather_than_overwriting() {
        let t = Harness::new();
        std::fs::create_dir_all(t.home.path().join(".zshrc")).unwrap();

        let outcome = t.apply();
        assert!(outcome.skipped.iter().any(|s| s.contains(".zshrc")));
        assert!(
            t.home.path().join(".zshrc").is_dir(),
            "must not be replaced"
        );
    }

    #[test]
    fn hooks_run_and_are_recorded_so_they_settle() {
        let t = Harness::new();
        let first = t.apply();
        assert_eq!(first.hooks_run, vec!["probe"]);
        assert!(t.repo.path().join("hook-ran.txt").is_file());

        let second = t.apply();
        assert!(
            second.hooks_run.is_empty(),
            "a once hook must not run twice"
        );
    }

    #[test]
    fn only_files_skips_hooks_and_hooks_only_skips_files() {
        let t = Harness::new();
        let files = t.apply_with(ApplyOptions {
            only_files: true,
            ..Default::default()
        });
        assert!(files.hooks_run.is_empty());
        assert_eq!(files.created, 2);
        assert_eq!(files.scaffolded, 7);

        let t2 = Harness::new();
        let hooks = t2.apply_with(ApplyOptions {
            hooks_only: true,
            ..Default::default()
        });
        assert_eq!(hooks.created, 0);
        assert_eq!(hooks.scaffolded, 0);
        assert_eq!(hooks.hooks_run, vec!["probe"]);
        assert!(!t2.home.path().join(".zshrc").exists());
    }

    /// A hook that fails must leave the already-written files in place and the
    /// state consistent, so re-running continues rather than redoing.
    #[test]
    fn a_failing_hook_leaves_files_applied_and_state_saved() {
        let t = Harness::new();
        t.break_hook();

        let err = t.try_apply().unwrap_err();
        assert!(err.to_string().contains("probe"), "{err}");
        assert!(
            t.home.path().join(".zshrc").is_file(),
            "files should still be written"
        );

        // The next plan sees no file work left, only the hook.
        let p = t.plan();
        assert_eq!(p.summary.add, 0);
        assert_eq!(p.summary.hooks, 1);
    }

    #[test]
    fn scaffolds_are_created_once_and_never_recorded() {
        let t = Harness::new();
        let first = t.apply();
        assert_eq!(first.scaffolded, 7);
        assert!(first.line().contains("7 scaffolded"));

        let conf = t.home.path().join(".ssh/config.d/normal.conf");
        let text = std::fs::read_to_string(&conf).unwrap();
        assert!(
            text.starts_with("# ssh hosts and keys for hat normal"),
            "{text}"
        );
        assert!(
            t.state().file(&conf).is_none(),
            "hats does not own a scaffold"
        );

        std::fs::write(&conf, "Host mine\n").unwrap();
        let second = t.apply();
        assert_eq!(second.scaffolded, 0);
        assert_eq!(std::fs::read_to_string(&conf).unwrap(), "Host mine\n");
        assert_eq!(t.plan().summary.scaffold, 0);
    }

    #[test]
    fn a_removed_hat_keeps_its_files() {
        let t = Harness::new();
        t.apply();
        t.local.borrow_mut().hats.shift_remove("normal");

        let p = t.plan();
        assert_eq!(p.summary.destroy, 0);
        assert!(p.scaffolds.is_empty());
        t.apply();
        assert!(t.home.path().join(".gitconfig.d/normal").is_file());
        assert!(t.home.path().join(".ssh/config.d/normal.conf").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn a_hats_k9s_directory_links_to_the_theme() {
        let t = Harness::new();
        let k9s = t.home.path().join(".config/k9s");
        std::fs::create_dir_all(&k9s).unwrap();
        std::fs::write(k9s.join("config.yaml"), "k9s: {}\n").unwrap();

        t.apply();
        let link = k9s.join("hats/normal/config.yaml");
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            PathBuf::from("../../config.yaml")
        );
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "k9s: {}\n");
        assert!(
            k9s.join("hats/normal/skins").symlink_metadata().is_err(),
            "no link to a theme that is not there"
        );
    }

    #[test]
    fn writing_is_atomic_and_leaves_no_temp_files() {
        let t = Harness::new();
        t.apply();
        let leftovers: Vec<_> = walkdir::WalkDir::new(t.home.path())
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("hats-tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn a_template_is_rendered_with_the_platform_values() {
        let t = Harness::new();
        t.apply();
        let written = std::fs::read_to_string(t.home.path().join(".zshrc")).unwrap();
        assert!(
            written.contains("/opt/homebrew"),
            "brew prefix not rendered: {written}"
        );
        assert!(
            !written.contains("{{"),
            "template markers left behind: {written}"
        );
    }
}
