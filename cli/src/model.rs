//! Expanding the manifest's file list into concrete managed files.
//!
//! A `FileSpec` in `hats.yaml` may name a single file or a whole directory.
//! This module turns both into a flat list of [`ManagedFile`]s, each with an
//! absolute source, an absolute target, and the mode it should end up with.
//! Everything downstream (plan, apply, render, lint) works from that list, so
//! the "is it a directory?" question is answered exactly once.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::Config;
use crate::config::repo::FileSpec;
use crate::platform::Platform;

/// One file hats owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedFile {
    /// Absolute path inside the repo's `files/` directory.
    pub source: PathBuf,
    /// Path under `$HOME`, with any `.j2` suffix already stripped.
    pub rel: PathBuf,
    /// Absolute destination.
    pub target: PathBuf,
    /// Octal mode to set, or None to use the default (0644).
    pub mode: Option<u32>,
    /// Octal mode for directories created along the way.
    pub dir_mode: Option<u32>,
    /// Rendered through the template engine rather than copied.
    pub template: bool,
    /// Diff is withheld unless `--show-secrets`.
    pub secret: bool,
    pub group: String,
}

impl ManagedFile {
    /// How the target is shown in plans: `~/...` rather than a long absolute
    /// path, because the home prefix is the same on every line.
    pub fn display(&self, home: &Path) -> String {
        match self.target.strip_prefix(home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => self.target.display().to_string(),
        }
    }

    pub fn mode_or_default(&self) -> u32 {
        self.mode.unwrap_or(0o644)
    }
}

/// Filters applied to the expansion, from `--target` and `--group`.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Only files whose target ends with one of these paths.
    pub targets: Vec<PathBuf>,
    /// Only files in these groups.
    pub groups: Vec<String>,
}

impl Filter {
    fn allows(&self, f: &ManagedFile) -> bool {
        let group_ok = self.groups.is_empty() || self.groups.contains(&f.group);
        let target_ok = self.targets.is_empty()
            || self
                .targets
                .iter()
                .any(|t| f.target.ends_with(t) || f.rel.ends_with(t) || f.target == *t);
        group_ok && target_ok
    }
}

/// Expand every enabled, platform-matching file spec into concrete files.
///
/// Disabled groups and non-matching platforms are dropped here, which is why
/// `plan` never has to ask whether a file applies: if it is in the list, it
/// applies.
pub fn expand(
    cfg: &Config,
    files_dir: &Path,
    platform: &Platform,
    filter: &Filter,
) -> Result<Vec<ManagedFile>> {
    let mut out = Vec::new();

    for spec in &cfg.repo.files {
        if !cfg.group_enabled(&spec.group) || !spec.condition.matches(platform) {
            continue;
        }
        let source = files_dir.join(&spec.path);
        if !source.exists() {
            // A manifest entry with no file is a repo bug, not a user error.
            // Reported by `hats lint`; skipped here so a partial migration
            // still plans.
            continue;
        }
        if source.is_dir() {
            expand_dir(spec, &source, files_dir, platform, &mut out)?;
        } else {
            out.push(build(spec, source, spec.target_rel(), platform));
        }
    }

    out.retain(|f| filter.allows(f));
    // Deterministic order so plans and snapshots are stable.
    out.sort_by(|a, b| a.target.cmp(&b.target));
    Ok(out)
}

fn expand_dir(
    spec: &FileSpec,
    dir: &Path,
    files_dir: &Path,
    platform: &Platform,
    out: &mut Vec<ManagedFile>,
) -> Result<()> {
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.with_context(|| format!("walking {}", dir.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let source = entry.path().to_path_buf();
        let rel_in_repo = source
            .strip_prefix(files_dir)
            .expect("walked path is always under files_dir");
        let rel = match rel_in_repo.to_str() {
            Some(s) => PathBuf::from(s.strip_suffix(".j2").unwrap_or(s)),
            None => rel_in_repo.to_path_buf(),
        };
        out.push(build(spec, source, rel, platform));
    }
    Ok(())
}

fn build(spec: &FileSpec, source: PathBuf, rel: PathBuf, platform: &Platform) -> ManagedFile {
    let template = source.extension().is_some_and(|e| e == "j2");
    ManagedFile {
        target: platform.home.join(&rel),
        rel,
        source,
        mode: spec.mode.as_deref().and_then(parse_mode),
        dir_mode: spec.dir_mode.as_deref().and_then(parse_mode),
        template,
        secret: spec.secret,
        group: spec.group.clone(),
    }
}

/// Parse an octal mode string like "0600". Returns None for nonsense, which
/// `hats lint` reports; a bad mode must not silently become 0000.
pub fn parse_mode(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim_start_matches("0o"), 8)
        .ok()
        .filter(|m| *m <= 0o7777)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::local::LocalConfig;
    use crate::config::repo::RepoConfig;
    use crate::platform::{Arch, Os};

    fn platform(home: &Path) -> Platform {
        Platform {
            os: Os::Darwin,
            arch: Arch::Arm64,
            home: home.to_path_buf(),
            user: "t".into(),
            hostname: "h".into(),
            brew_prefix: PathBuf::from("/opt/homebrew"),
        }
    }

    /// A files/ tree with a template, a plain file, a directory and a
    /// platform-gated file.
    fn repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().join("files");
        std::fs::create_dir_all(files.join(".config/bat/themes")).unwrap();
        std::fs::create_dir_all(files.join(".ssh")).unwrap();
        std::fs::write(files.join(".zshrc.j2"), "zshrc").unwrap();
        std::fs::write(files.join(".ssh/config.j2"), "sshconfig").unwrap();
        std::fs::write(files.join(".config/bat/config"), "batconf").unwrap();
        std::fs::write(files.join(".config/bat/themes/Mocha.tmTheme"), "theme").unwrap();
        std::fs::write(files.join(".mac-only"), "mac").unwrap();
        (dir, files)
    }

    const MANIFEST: &str = r#"
groups:
  shell: { description: s }
  ssh:   { description: s }
  theme: { description: t }
  mac:   { description: m }
files:
  - { path: .zshrc.j2, group: shell }
  - { path: .ssh/config.j2, group: ssh, mode: "0600", dir_mode: "0700" }
  - { path: .config/bat, group: theme }
  - { path: .mac-only, group: mac, os: [darwin] }
"#;

    fn config(local: &str) -> Config {
        Config {
            repo: serde_yaml_ng::from_str::<RepoConfig>(MANIFEST).unwrap(),
            local: serde_yaml_ng::from_str::<LocalConfig>(local).unwrap(),
        }
    }

    fn expand_all(local: &str, home: &Path, files: &Path) -> Vec<ManagedFile> {
        expand(&config(local), files, &platform(home), &Filter::default()).unwrap()
    }

    #[test]
    fn a_directory_expands_to_every_file_under_it() {
        let (_d, files) = repo();
        let home = Path::new("/home/t");
        let got = expand_all("groups: {}", home, &files);
        let targets: Vec<String> = got.iter().map(|f| f.display(home)).collect();
        assert!(targets.contains(&"~/.config/bat/config".to_string()));
        assert!(targets.contains(&"~/.config/bat/themes/Mocha.tmTheme".to_string()));
    }

    #[test]
    fn the_j2_suffix_marks_a_template_and_leaves_the_target() {
        let (_d, files) = repo();
        let got = expand_all("groups: {}", Path::new("/home/t"), &files);
        let zshrc = got.iter().find(|f| f.rel == Path::new(".zshrc")).unwrap();
        assert!(zshrc.template);
        assert_eq!(zshrc.target, Path::new("/home/t/.zshrc"));

        let bat = got
            .iter()
            .find(|f| f.rel == Path::new(".config/bat/config"))
            .unwrap();
        assert!(!bat.template, "a plain file is copied, not rendered");
    }

    #[test]
    fn modes_are_parsed_from_octal() {
        let (_d, files) = repo();
        let got = expand_all("groups: {}", Path::new("/home/t"), &files);
        let ssh = got
            .iter()
            .find(|f| f.rel == Path::new(".ssh/config"))
            .unwrap();
        assert_eq!(ssh.mode, Some(0o600));
        assert_eq!(ssh.dir_mode, Some(0o700));
        assert_eq!(ssh.mode_or_default(), 0o600);

        let zshrc = got.iter().find(|f| f.rel == Path::new(".zshrc")).unwrap();
        assert_eq!(zshrc.mode, None);
        assert_eq!(zshrc.mode_or_default(), 0o644);
    }

    #[test]
    fn parse_mode_rejects_nonsense_rather_than_defaulting_to_zero() {
        assert_eq!(parse_mode("0600"), Some(0o600));
        assert_eq!(parse_mode("700"), Some(0o700));
        assert_eq!(parse_mode("0o644"), Some(0o644));
        assert_eq!(parse_mode("rwx"), None);
        assert_eq!(parse_mode("0999"), None, "9 is not an octal digit");
        assert_eq!(parse_mode("77777"), None, "wider than a mode");
    }

    #[test]
    fn a_disabled_group_contributes_nothing() {
        let (_d, files) = repo();
        let got = expand_all("groups: { theme: false }", Path::new("/home/t"), &files);
        assert!(!got.iter().any(|f| f.group == "theme"));
        assert!(got.iter().any(|f| f.group == "shell"));
    }

    #[test]
    fn a_platform_gate_drops_the_file_rather_than_managing_it() {
        let (_d, files) = repo();
        let home = Path::new("/home/t");
        let cfg = config("groups: {}");

        let mut linux = platform(home);
        linux.os = Os::Linux;
        let on_linux = expand(&cfg, &files, &linux, &Filter::default()).unwrap();
        assert!(!on_linux.iter().any(|f| f.group == "mac"));

        let on_mac = expand(&cfg, &files, &platform(home), &Filter::default()).unwrap();
        assert!(on_mac.iter().any(|f| f.group == "mac"));
    }

    #[test]
    fn a_manifest_entry_with_no_file_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().join("files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join(".zshrc.j2"), "x").unwrap();
        let got = expand_all("groups: {}", Path::new("/home/t"), &files);
        assert_eq!(got.len(), 1, "only the file that exists");
    }

    #[test]
    fn output_is_sorted_so_plans_are_stable() {
        let (_d, files) = repo();
        let got = expand_all("groups: {}", Path::new("/home/t"), &files);
        let targets: Vec<_> = got.iter().map(|f| f.target.clone()).collect();
        let mut sorted = targets.clone();
        sorted.sort();
        assert_eq!(targets, sorted);
    }

    #[test]
    fn a_group_filter_narrows_the_list() {
        let (_d, files) = repo();
        let filter = Filter {
            groups: vec!["ssh".into()],
            targets: vec![],
        };
        let got = expand(
            &config("groups: {}"),
            &files,
            &platform(Path::new("/home/t")),
            &filter,
        )
        .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].rel, Path::new(".ssh/config"));
    }

    #[test]
    fn a_target_filter_accepts_a_relative_or_absolute_path() {
        let (_d, files) = repo();
        let home = Path::new("/home/t");
        for target in [PathBuf::from(".zshrc"), PathBuf::from("/home/t/.zshrc")] {
            let filter = Filter {
                targets: vec![target.clone()],
                groups: vec![],
            };
            let got = expand(&config("groups: {}"), &files, &platform(home), &filter).unwrap();
            assert_eq!(got.len(), 1, "filtering by {}", target.display());
            assert_eq!(got[0].rel, Path::new(".zshrc"));
        }
    }

    #[test]
    fn display_shortens_the_home_prefix() {
        let (_d, files) = repo();
        let home = Path::new("/home/t");
        let got = expand_all("groups: {}", home, &files);
        assert!(got.iter().all(|f| f.display(home).starts_with("~/")));
    }
}
