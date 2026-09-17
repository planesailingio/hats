//! A folder tree that names its hat.
//!
//! A `.hat` file holds one hat name. `hats env --here` finds the nearest one at
//! or above the current directory, and the shell integration runs that on every
//! `cd`, so working under `~/work/acme` puts the acme hat on without anyone
//! remembering to.
//!
//! The file holds a name and nothing else. There is no code in it to trust, so
//! there is no allow step: the worst a `.hat` in a cloned repository can do is
//! select one of the hats this machine already has, and the switch is announced.
//!
//! Two variables carry the state between runs. `HATS_HAT_FILE` is the `.hat`
//! file in effect, and `HATS_HAT_PREV` is the hat that was on when a folder
//! first took over. A switch happens only when the file in effect changes,
//! which is what lets a `hat <name>` typed inside a tree stick until the shell
//! leaves it. Neither variable is in the reset set, for the same reason: a
//! manual switch must leave them alone.
//!
//! Everything here runs before the configuration is loaded, because on most
//! `cd`s there is nothing to do and the shell is waiting.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::scaffold::check_name;

/// The file a folder names its hat in.
pub const FILE: &str = ".hat";
/// The `.hat` file in effect in this shell.
pub const FILE_VAR: &str = "HATS_HAT_FILE";
/// The hat that was on when a `.hat` file first took over.
pub const PREV_VAR: &str = "HATS_HAT_PREV";

/// What this shell says about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shell {
    pub hat: Option<String>,
    pub file: Option<PathBuf>,
    pub prev: Option<String>,
}

impl Shell {
    /// Read the shell's state from the environment. Empty counts as unset.
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|s| !s.is_empty());
        Self {
            hat: var("HATS_HAT"),
            file: var(FILE_VAR).map(PathBuf::from),
            prev: var(PREV_VAR),
        }
    }
}

/// What a change of directory asks of the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The same file is in effect as before, or none was and none is.
    Nothing,
    /// A `.hat` file has come into effect.
    Enter {
        file: PathBuf,
        hat: String,
        /// The hat to go back to on leaving, if one was on.
        prev: Option<String>,
    },
    /// The shell has left every tree; go back to `prev`, if it is known.
    Leave { prev: Option<String> },
}

/// The nearest `.hat` file at or above `start`.
pub fn find(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(FILE))
        .find(|path| path.is_file())
}

/// The hat a `.hat` file names: its first line that is not blank or a comment.
pub fn read(path: &Path) -> Result<String> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let Some(name) = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
    else {
        bail!("{} does not name a hat", path.display());
    };
    check_name(name).with_context(|| format!("in {}", path.display()))?;
    Ok(name.to_string())
}

/// Decide what moving to `cwd` means for a shell in state `shell`.
pub fn decide(cwd: &Path, shell: &Shell) -> Result<Step> {
    let found = find(cwd);
    if found == shell.file {
        return Ok(Step::Nothing);
    }
    Ok(match found {
        Some(file) => Step::Enter {
            hat: read(&file)?,
            // Straight from one tree into another, the hat to go back to is
            // still the one from before the first tree.
            prev: if shell.file.is_some() {
                shell.prev.clone()
            } else {
                shell.hat.clone()
            },
            file,
        },
        None => Step::Leave {
            prev: shell.prev.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        // Canonical, as `current_dir` would report it: /tmp is a symlink on macOS.
        let root = dir.path().canonicalize().unwrap();
        (dir, root)
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn the_nearest_file_wins() {
        let (_g, root) = tree();
        write(&root.join(".hat"), "outer\n");
        write(&root.join("a/.hat"), "inner\n");
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::create_dir_all(root.join("z")).unwrap();

        assert_eq!(find(&root.join("a/b/c")), Some(root.join("a/.hat")));
        assert_eq!(find(&root.join("a")), Some(root.join("a/.hat")));
        assert_eq!(find(&root.join("z")), Some(root.join(".hat")));
    }

    #[test]
    fn a_directory_named_dot_hat_is_not_a_hat_file() {
        let (_g, root) = tree();
        write(&root.join(".hat"), "outer\n");
        std::fs::create_dir_all(root.join("a/.hat")).unwrap();

        assert_eq!(find(&root.join("a")), Some(root.join(".hat")));
    }

    #[test]
    fn the_name_is_the_first_line_that_is_not_blank_or_a_comment() {
        let (_g, root) = tree();
        let file = root.join(".hat");

        write(&file, "\n# which hat this tree wears\n  acme  \nignored\n");
        assert_eq!(read(&file).unwrap(), "acme");

        write(&file, "acme");
        assert_eq!(read(&file).unwrap(), "acme", "no trailing newline");
    }

    #[test]
    fn a_file_that_names_nothing_usable_is_an_error() {
        let (_g, root) = tree();
        let file = root.join(".hat");

        write(&file, "# nothing here\n\n");
        assert!(read(&file).is_err());

        write(&file, "../escape\n");
        let err = format!("{:#}", read(&file).unwrap_err());
        assert!(err.contains(".hat"), "the error names the file: {err}");
    }

    #[test]
    fn outside_every_tree_there_is_nothing_to_do() {
        let (_g, root) = tree();
        let shell = Shell {
            hat: Some("home".into()),
            ..Shell::default()
        };
        // Only meaningful where no `.hat` sits above the temp directory.
        if find(&root).is_none() {
            assert_eq!(decide(&root, &shell).unwrap(), Step::Nothing);
        }
    }

    #[test]
    fn entering_a_tree_remembers_the_hat_that_was_on() {
        let (_g, root) = tree();
        write(&root.join(".hat"), "acme\n");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let shell = Shell {
            hat: Some("home".into()),
            ..Shell::default()
        };

        assert_eq!(
            decide(&root.join("sub"), &shell).unwrap(),
            Step::Enter {
                file: root.join(".hat"),
                hat: "acme".into(),
                prev: Some("home".into()),
            }
        );
    }

    #[test]
    fn moving_within_a_tree_leaves_a_manual_choice_alone() {
        let (_g, root) = tree();
        write(&root.join(".hat"), "acme\n");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        // `hat other` was typed inside the tree: the file in effect is unchanged.
        let shell = Shell {
            hat: Some("other".into()),
            file: Some(root.join(".hat")),
            prev: Some("home".into()),
        };

        assert_eq!(decide(&root.join("sub"), &shell).unwrap(), Step::Nothing);
    }

    #[test]
    fn crossing_into_another_tree_keeps_the_original_previous_hat() {
        let (_g, root) = tree();
        write(&root.join("a/.hat"), "acme\n");
        write(&root.join("b/.hat"), "globex\n");
        let shell = Shell {
            hat: Some("acme".into()),
            file: Some(root.join("a/.hat")),
            prev: Some("home".into()),
        };

        assert_eq!(
            decide(&root.join("b"), &shell).unwrap(),
            Step::Enter {
                file: root.join("b/.hat"),
                hat: "globex".into(),
                prev: Some("home".into()),
            }
        );
    }

    #[test]
    fn leaving_a_tree_goes_back_to_the_previous_hat() {
        let (_g, root) = tree();
        if find(&root).is_some() {
            return;
        }
        let shell = Shell {
            hat: Some("acme".into()),
            file: Some(root.join("gone/.hat")),
            prev: Some("home".into()),
        };

        assert_eq!(
            decide(&root, &shell).unwrap(),
            Step::Leave {
                prev: Some("home".into())
            }
        );
    }

    #[test]
    fn a_broken_file_is_an_error_not_a_switch() {
        let (_g, root) = tree();
        write(&root.join(".hat"), "not a name\n");

        assert!(decide(&root, &Shell::default()).is_err());
    }
}
