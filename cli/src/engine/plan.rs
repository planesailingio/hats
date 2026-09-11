//! Working out what an apply would change, without changing anything.
//!
//! This is the half chezmoi never gave: before writing, say exactly which files
//! appear, which change and how, which permissions move, and which files hats
//! used to own but no longer does.
//!
//! `plan` is also the single code path behind `apply` (which re-plans), `diff`
//! and `render`, so what you are shown and what happens cannot diverge.

use std::path::PathBuf;

use anyhow::Result;

use crate::config::Config;
use crate::engine::diff::{self, Body, Stats};
use crate::engine::redact::Redactor;
use crate::engine::render::{RedactMode, RenderContext, Renderer};
use crate::engine::state::{self, State};
use crate::hat::scaffold::{self, Scaffold};
use crate::hooks::{self, HookPlan};
use crate::model::{Filter, ManagedFile};
use crate::platform::Platform;
use crate::secrets::store::Secrets;

/// What will happen to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Not there yet.
    Create,
    /// Content differs.
    Update,
    /// Content matches; only the mode is wrong.
    ModeChange { from: u32, to: u32 },
    /// Already correct.
    Unchanged,
    /// Managed before, not managed now. `safe` means it is unmodified since
    /// hats wrote it, so removing it loses nothing.
    Destroy { safe: bool },
    /// Something is in the way that hats will not overwrite blindly.
    Conflict { why: String },
}

impl Action {
    /// The single character that opens the line, in the style of a terraform
    /// plan.
    pub fn marker(&self) -> char {
        match self {
            Action::Create => '+',
            Action::Update => '~',
            Action::ModeChange { .. } => '!',
            Action::Unchanged => '=',
            Action::Destroy { .. } => '-',
            Action::Conflict { .. } => '×',
        }
    }

    pub fn label(&self) -> String {
        match self {
            Action::Create => "create".into(),
            Action::Update => "update".into(),
            Action::ModeChange { from, to } => format!("mode {from:04o} → {to:04o}"),
            Action::Unchanged => "unchanged".into(),
            Action::Destroy { safe: true } => "destroy".into(),
            Action::Destroy { safe: false } => "destroy (modified locally)".into(),
            Action::Conflict { why } => format!("conflict: {why}"),
        }
    }

    pub fn is_change(&self) -> bool {
        !matches!(self, Action::Unchanged)
    }
}

/// One line of the plan.
#[derive(Debug, Clone)]
pub struct Entry {
    pub target: PathBuf,
    /// `~/.zshrc` rather than the full path.
    pub display: String,
    pub action: Action,
    pub stats: Stats,
    pub body: Body,
    /// Rendered content to write. None for destroys and conflicts.
    pub contents: Option<Vec<u8>>,
    pub mode: u32,
    pub dir_mode: Option<u32>,
    pub secret: bool,
}

/// Counts for the summary line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub add: usize,
    pub change: usize,
    pub destroy: usize,
    pub mode: usize,
    pub unchanged: usize,
    pub conflict: usize,
    pub scaffold: usize,
    pub hooks: usize,
}

impl Summary {
    pub fn has_changes(&self) -> bool {
        self.add
            + self.change
            + self.destroy
            + self.mode
            + self.conflict
            + self.scaffold
            + self.hooks
            > 0
    }

    /// The terraform-style one-liner.
    pub fn line(&self) -> String {
        format!(
            "Plan: {} to add, {} to change, {} to destroy, {} permission change{}, {} to scaffold; {} hook{} to run.",
            self.add,
            self.change,
            self.destroy,
            self.mode,
            if self.mode == 1 { "" } else { "s" },
            self.scaffold,
            self.hooks,
            if self.hooks == 1 { "" } else { "s" },
        )
    }
}

/// Everything an apply would do.
#[derive(Debug)]
pub struct Plan {
    pub entries: Vec<Entry>,
    /// Per-hat files to create once. Not managed: see [`scaffold`].
    pub scaffolds: Vec<Scaffold>,
    pub hooks: Vec<HookPlan>,
    pub summary: Summary,
    /// Secrets referenced anywhere but not fetched.
    pub missing_secrets: Vec<String>,
    /// Notes for the plan header (no secrets fetched, repo untagged, ...).
    pub notes: Vec<String>,
}

/// Inputs to planning.
pub struct PlanOptions<'a> {
    pub cfg: &'a Config,
    pub platform: &'a Platform,
    pub secrets: &'a Secrets,
    pub state: &'a State,
    pub repo_dir: PathBuf,
    pub files_dir: PathBuf,
    pub hats_home: PathBuf,
    pub filter: Filter,
    pub skip_hooks: bool,
    pub show_secrets: bool,
    pub force_hook: Option<String>,
}

pub fn plan(opts: &PlanOptions<'_>) -> Result<Plan> {
    let files = crate::model::expand(opts.cfg, &opts.files_dir, opts.platform, &opts.filter)?;

    let ctx = RenderContext::build(
        opts.cfg,
        opts.platform,
        opts.secrets,
        &opts.repo_dir,
        &opts.files_dir,
        RedactMode::Real,
    );
    let renderer = Renderer::new(&ctx);
    let redactor = if opts.show_secrets {
        Redactor::disabled()
    } else {
        Redactor::new(opts.secrets)
    };

    let mut entries = Vec::new();
    let mut summary = Summary::default();

    for file in &files {
        let entry = classify(&renderer, file, opts, &redactor)?;
        match &entry.action {
            Action::Create => summary.add += 1,
            Action::Update => summary.change += 1,
            Action::ModeChange { .. } => summary.mode += 1,
            Action::Unchanged => summary.unchanged += 1,
            Action::Conflict { .. } => summary.conflict += 1,
            Action::Destroy { .. } => summary.destroy += 1,
        }
        entries.push(entry);
    }

    // Files hats owned last time but does not now: a disabled group, or a file
    // removed upstream. Only detectable from state.
    let managed: Vec<PathBuf> = files.iter().map(|f| f.target.clone()).collect();
    let unfiltered = opts.filter.targets.is_empty() && opts.filter.groups.is_empty();
    if unfiltered {
        for orphan in opts.state.orphans(&managed) {
            let recorded = opts.state.file(&orphan);
            let current = std::fs::read(&orphan).ok();
            let safe = match (recorded, &current) {
                // Gone already: nothing to do, so do not list it.
                (_, None) => continue,
                (Some(r), Some(bytes)) => r.hash == state::hash(bytes),
                (None, Some(_)) => false,
            };
            summary.destroy += 1;
            entries.push(Entry {
                display: display_path(&orphan, opts.platform),
                target: orphan,
                action: Action::Destroy { safe },
                stats: Stats::default(),
                body: Body::None,
                contents: None,
                mode: 0,
                dir_mode: None,
                secret: false,
            });
        }
    }

    entries.sort_by(|a, b| a.target.cmp(&b.target));

    // Like orphans, only on an unfiltered run: `--target .zshrc` must not
    // create a hat's ssh file.
    let scaffolds = if unfiltered {
        scaffold::wanted(opts.cfg, &opts.platform.home, &files)
    } else {
        Vec::new()
    };
    summary.scaffold = scaffolds.len();

    let hooks = if opts.skip_hooks {
        Vec::new()
    } else {
        hooks::due(
            &opts.cfg.repo.hooks,
            &opts.repo_dir,
            opts.platform,
            opts.state,
            opts.force_hook.as_deref(),
        )
    };
    summary.hooks = hooks.len();

    // Every secret any hat refers to, plus the ones the repo declares.
    let mut wanted: std::collections::BTreeSet<String> =
        opts.cfg.repo.secrets.required.iter().cloned().collect();
    for name in opts.cfg.hats().keys() {
        if let Ok(p) = opts.cfg.resolve_hat(name) {
            wanted.extend(p.secret_refs());
        }
    }
    let missing_secrets = opts.secrets.missing(wanted.iter().map(String::as_str));

    let mut notes = Vec::new();
    if !missing_secrets.is_empty() {
        notes.push(format!(
            "{} secret{} not fetched: {}. Templates render them empty.",
            missing_secrets.len(),
            if missing_secrets.len() == 1 { "" } else { "s" },
            missing_secrets.join(", ")
        ));
    }
    if opts.show_secrets {
        notes.push("--show-secrets: real values are printed below.".into());
    }

    Ok(Plan {
        entries,
        scaffolds,
        hooks,
        summary,
        missing_secrets,
        notes,
    })
}

fn classify(
    renderer: &Renderer<'_>,
    file: &ManagedFile,
    opts: &PlanOptions<'_>,
    redactor: &Redactor,
) -> Result<Entry> {
    let contents = renderer.render(file)?;
    let want_mode = file.mode_or_default();
    let display = file.display(&opts.platform.home);

    // A directory or symlink where a file should be is never silently replaced.
    let meta = std::fs::symlink_metadata(&file.target).ok();
    if let Some(m) = &meta {
        let why = if m.is_dir() {
            Some("a directory is in the way")
        } else if m.file_type().is_symlink() {
            Some("the target is a symlink")
        } else {
            None
        };
        if let Some(why) = why {
            return Ok(Entry {
                target: file.target.clone(),
                display,
                action: Action::Conflict { why: why.into() },
                stats: Stats::default(),
                body: Body::None,
                contents: None,
                mode: want_mode,
                dir_mode: file.dir_mode,
                secret: file.secret,
            });
        }
    }

    let Some(existing) = std::fs::read(&file.target).ok() else {
        let lines = std::str::from_utf8(&contents)
            .map(|t| t.lines().count())
            .unwrap_or(0);
        return Ok(Entry {
            target: file.target.clone(),
            display,
            action: Action::Create,
            stats: Stats {
                added: lines,
                removed: 0,
            },
            body: Body::Whole { lines },
            contents: Some(contents),
            mode: want_mode,
            dir_mode: file.dir_mode,
            secret: file.secret,
        });
    };

    let have_mode = current_mode(&file.target).unwrap_or(want_mode);

    if existing == contents {
        let action = if have_mode == want_mode {
            Action::Unchanged
        } else {
            Action::ModeChange {
                from: have_mode,
                to: want_mode,
            }
        };
        return Ok(Entry {
            target: file.target.clone(),
            display,
            action,
            stats: Stats::default(),
            body: Body::None,
            contents: Some(contents),
            mode: want_mode,
            dir_mode: file.dir_mode,
            secret: file.secret,
        });
    }

    // Mask before diffing, on both sides: the old file may hold a rotated value
    // the redactor knows nothing about, which is why `secret: true` files also
    // withhold their body entirely.
    let old_masked = redactor.mask_bytes(&existing);
    let new_masked = redactor.mask_bytes(&contents);
    let stats = match (
        std::str::from_utf8(&old_masked),
        std::str::from_utf8(&new_masked),
    ) {
        (Ok(o), Ok(n)) => Stats::of(o, n),
        _ => Stats::default(),
    };

    Ok(Entry {
        target: file.target.clone(),
        display,
        action: Action::Update,
        stats,
        body: diff::body(&old_masked, &new_masked, file.secret && !opts.show_secrets),
        contents: Some(contents),
        mode: want_mode,
        dir_mode: file.dir_mode,
        secret: file.secret,
    })
}

fn display_path(path: &std::path::Path, platform: &Platform) -> String {
    match path.strip_prefix(&platform.home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(unix)]
fn current_mode(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn current_mode(_path: &std::path::Path) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testkit::*;

    #[test]
    fn a_fresh_home_is_all_creates() {
        let t = Harness::new();
        let p = t.plan();
        assert_eq!(p.summary.add, 2);
        assert_eq!(p.summary.change, 0);
        assert!(p.summary.has_changes());
        assert!(p.summary.line().contains("2 to add"));
    }

    #[test]
    fn after_applying_the_plan_is_empty() {
        let t = Harness::new();
        t.apply();
        let p = t.plan();
        assert_eq!(p.summary.add, 0);
        assert_eq!(p.summary.unchanged, 2);
        assert!(!p.summary.has_changes());
    }

    #[test]
    fn editing_a_target_shows_an_update_with_a_line_diff() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".zshrc"), "hand edited\n").unwrap();

        let p = t.plan();
        assert_eq!(p.summary.change, 1);
        let e = t.entry(&p, "~/.zshrc");
        assert_eq!(e.action, Action::Update);
        match &e.body {
            Body::Unified(d) => {
                assert!(d.contains("-hand edited"), "{d}");
                assert!(d.contains('+'), "{d}");
            }
            other => panic!("expected a unified diff, got {other:?}"),
        }
    }

    #[test]
    fn a_wrong_mode_alone_is_reported_as_a_permission_change() {
        let t = Harness::new();
        t.apply();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let ssh = t.home.path().join(".ssh/config");
            std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o644)).unwrap();

            let p = t.plan();
            assert_eq!(p.summary.mode, 1);
            let e = t.entry(&p, "~/.ssh/config");
            assert_eq!(
                e.action,
                Action::ModeChange {
                    from: 0o644,
                    to: 0o600
                }
            );
            assert!(e.action.label().contains("0600"));
        }
    }

    #[test]
    fn disabling_a_group_turns_its_files_into_destroys() {
        let t = Harness::new();
        t.apply();
        t.set_group("ssh", false);

        let p = t.plan();
        assert_eq!(p.summary.destroy, 1);
        let e = t.entry(&p, "~/.ssh/config");
        assert_eq!(e.action, Action::Destroy { safe: true });
    }

    /// A file the user has edited must never be quietly removed.
    #[test]
    fn a_locally_modified_orphan_is_flagged_as_unsafe() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".ssh/config"), "mine now\n").unwrap();
        t.set_group("ssh", false);

        let plan = t.plan();
        let e = t.entry(&plan, "~/.ssh/config");
        assert_eq!(e.action, Action::Destroy { safe: false });
        assert!(e.action.label().contains("modified locally"));
    }

    #[test]
    fn an_orphan_already_deleted_by_hand_is_not_listed() {
        let t = Harness::new();
        t.apply();
        std::fs::remove_file(t.home.path().join(".ssh/config")).unwrap();
        t.set_group("ssh", false);
        assert_eq!(t.plan().summary.destroy, 0);
    }

    #[test]
    fn a_directory_in_the_way_is_a_conflict_not_an_overwrite() {
        let t = Harness::new();
        std::fs::create_dir_all(t.home.path().join(".zshrc")).unwrap();
        let p = t.plan();
        assert_eq!(p.summary.conflict, 1);
        let e = t.entry(&p, "~/.zshrc");
        assert!(matches!(e.action, Action::Conflict { .. }));
        assert!(
            e.contents.is_none(),
            "a conflict must not carry content to write"
        );
    }

    #[test]
    fn a_secret_file_withholds_its_diff_but_still_reports_counts() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".ssh/config"), "old token\n").unwrap();

        let plan = t.plan();
        let e = t.entry(&plan, "~/.ssh/config");
        assert!(matches!(e.body, Body::Withheld { .. }), "{:?}", e.body);
    }

    #[test]
    fn show_secrets_reveals_the_withheld_diff() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".ssh/config"), "old token\n").unwrap();

        let plan = t.plan_with(|o| o.show_secrets = true);
        let e = t.entry(&plan, "~/.ssh/config");
        assert!(matches!(e.body, Body::Unified(_)), "{:?}", e.body);
    }

    /// The value of a fetched secret must never appear in an ordinary plan.
    #[test]
    fn a_secret_value_never_reaches_the_diff() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".zshrc"), "TOKEN=stale\n").unwrap();

        let p = t.plan();
        let e = t.entry(&p, "~/.zshrc");
        if let Body::Unified(d) = &e.body {
            assert!(!d.contains("tok-secret-123"), "leaked the token:\n{d}");
            assert!(d.contains("«secret:jira_token»"), "should be masked:\n{d}");
        } else {
            panic!("expected a diff, got {:?}", e.body);
        }
    }

    #[test]
    fn a_target_filter_restricts_the_plan() {
        let t = Harness::new();
        let p = t.plan_with(|o| o.filter.targets = vec![PathBuf::from(".zshrc")]);
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].display, "~/.zshrc");
    }

    #[test]
    fn hooks_appear_in_the_plan_and_can_be_skipped() {
        let t = Harness::new();
        assert_eq!(t.plan().summary.hooks, 1);
        assert_eq!(t.plan_with(|o| o.skip_hooks = true).summary.hooks, 0);
    }

    #[test]
    fn unfetched_secrets_are_called_out_in_the_header() {
        let t = Harness::new();
        let p = t.plan_with(|o| o.secrets = &EMPTY_SECRETS);
        assert!(p.missing_secrets.contains(&"jira_token".to_string()));
        assert!(
            p.notes.iter().any(|n| n.contains("not fetched")),
            "{:?}",
            p.notes
        );
    }

    #[test]
    fn the_summary_line_reads_like_a_terraform_plan() {
        let s = Summary {
            add: 3,
            change: 2,
            destroy: 1,
            mode: 1,
            scaffold: 4,
            hooks: 2,
            ..Default::default()
        };
        assert_eq!(
            s.line(),
            "Plan: 3 to add, 2 to change, 1 to destroy, 1 permission change, 4 to scaffold; 2 hooks to run."
        );
    }

    #[test]
    fn every_hat_is_scaffolded_apart_from_the_managed_files() {
        let t = Harness::new();
        let p = t.plan();
        let got: Vec<&str> = p.scaffolds.iter().map(|s| s.display.as_str()).collect();
        assert_eq!(
            got,
            vec![
                "~/.aws/.hats/normal.config",
                "~/.aws/.hats/normal.credentials",
                "~/.config/coderv2/hats/normal",
                "~/.gitconfig.d/normal",
                "~/.kube/config.normal",
                "~/.ssh/config.d/common.conf",
                "~/.ssh/config.d/normal.conf",
            ]
        );
        assert_eq!(p.summary.scaffold, 7);
        assert_eq!(p.summary.add, 2, "a scaffold is not a managed file");
        assert!(p.summary.line().contains("7 to scaffold"));
    }

    #[test]
    fn a_filtered_plan_scaffolds_nothing() {
        let t = Harness::new();
        let p = t.plan_with(|o| o.filter.targets = vec![PathBuf::from(".zshrc")]);
        assert!(p.scaffolds.is_empty());
        let p = t.plan_with(|o| o.filter.groups = vec!["ssh".into()]);
        assert!(p.scaffolds.is_empty());
    }

    #[test]
    fn markers_are_distinct_per_action() {
        let markers = [
            Action::Create.marker(),
            Action::Update.marker(),
            Action::Destroy { safe: true }.marker(),
            Action::ModeChange { from: 0, to: 0 }.marker(),
            Action::Unchanged.marker(),
        ];
        let unique: std::collections::BTreeSet<_> = markers.iter().collect();
        assert_eq!(unique.len(), markers.len());
    }
}
