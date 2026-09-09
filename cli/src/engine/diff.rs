//! Unified, line-level diffs for the plan.
//!
//! This is the half of `hats plan` that answers "what exactly changes?", the
//! thing chezmoi's apply never showed before writing. Output follows the
//! familiar unified format so it reads like `git diff`.

use owo_colors::OwoColorize;
use similar::{ChangeTag, TextDiff};

/// Lines of context either side of a change.
const CONTEXT: usize = 3;

/// How much of a change to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// A unified diff.
    Unified(String),
    /// Content withheld because the file holds secrets.
    Withheld { added: usize, removed: usize },
    /// Not text, so a line diff would be meaningless.
    Binary { old: usize, new: usize },
    /// The whole file is new or gone; a diff would just be the file.
    Whole { lines: usize },
    /// Nothing to show.
    None,
}

/// Added and removed line counts, used for the one-line summary on each entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub added: usize,
    pub removed: usize,
}

impl Stats {
    pub fn of(old: &str, new: &str) -> Self {
        let diff = TextDiff::from_lines(old, new);
        let mut s = Self::default();
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => s.added += 1,
                ChangeTag::Delete => s.removed += 1,
                ChangeTag::Equal => {}
            }
        }
        s
    }

    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

/// Build the body to show for a change.
pub fn body(old: &[u8], new: &[u8], secret: bool) -> Body {
    let (Ok(old_text), Ok(new_text)) = (std::str::from_utf8(old), std::str::from_utf8(new)) else {
        return Body::Binary {
            old: old.len(),
            new: new.len(),
        };
    };

    let stats = Stats::of(old_text, new_text);
    if stats.is_empty() {
        return Body::None;
    }
    if secret {
        return Body::Withheld {
            added: stats.added,
            removed: stats.removed,
        };
    }
    Body::Unified(unified(old_text, new_text))
}

/// A unified diff with `CONTEXT` lines either side.
pub fn unified(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    for (i, group) in diff.grouped_ops(CONTEXT).iter().enumerate() {
        if i > 0 {
            out.push_str("...\n");
        }
        for op in group {
            for change in diff.iter_changes(op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                };
                out.push(sign);
                out.push_str(change.value());
                // A final line with no newline of its own would otherwise run
                // into the next diff line.
                if change.missing_newline() {
                    out.push('\n');
                }
            }
        }
    }
    out
}

/// Colourise a unified diff for the terminal. Kept separate from generation so
/// the same text can be written to a plan file without escape codes.
pub fn colourise(diff: &str) -> String {
    diff.lines()
        .map(|line| match line.chars().next() {
            Some('+') => line.green().to_string(),
            Some('-') => line.red().to_string(),
            Some('.') => line.dimmed().to_string(),
            _ => line.dimmed().to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Indent a body under its plan entry.
pub fn indent(text: &str, by: &str) -> String {
    text.lines()
        .map(|l| format!("{by}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_added_and_removed_lines() {
        let s = Stats::of("a\nb\nc\n", "a\nB\nc\nd\n");
        assert_eq!(s.added, 2);
        assert_eq!(s.removed, 1);
        assert!(!s.is_empty());
    }

    #[test]
    fn identical_content_has_no_stats_and_no_body() {
        assert!(Stats::of("same\n", "same\n").is_empty());
        assert_eq!(body(b"same\n", b"same\n", false), Body::None);
    }

    #[test]
    fn a_unified_diff_marks_both_sides_and_keeps_context() {
        let old = "one\ntwo\nthree\nfour\nfive\n";
        let new = "one\ntwo\nTHREE\nfour\nfive\n";
        let d = unified(old, new);
        assert!(d.contains("-three"), "{d}");
        assert!(d.contains("+THREE"), "{d}");
        assert!(d.contains(" one"), "context should be present:\n{d}");
    }

    #[test]
    fn distant_changes_are_separated_rather_than_dumping_the_file() {
        let old: String = (0..40).map(|i| format!("line{i}\n")).collect();
        let mut lines: Vec<String> = (0..40).map(|i| format!("line{i}\n")).collect();
        lines[1] = "CHANGED\n".into();
        lines[38] = "ALSO\n".into();
        let new: String = lines.concat();

        let d = unified(&old, &new);
        assert!(d.contains("..."), "hunks should be separated:\n{d}");
        assert!(
            !d.contains("line20"),
            "untouched middle should be omitted:\n{d}"
        );
    }

    #[test]
    fn a_secret_file_reports_counts_but_no_content() {
        let b = body(b"token=old\n", b"token=new\n", true);
        assert_eq!(
            b,
            Body::Withheld {
                added: 1,
                removed: 1
            }
        );
        // The value must not appear anywhere in the rendered body.
        assert!(!format!("{b:?}").contains("token=new"));
    }

    #[test]
    fn non_utf8_content_is_reported_as_binary() {
        let b = body(&[0xff, 0xfe], &[0xff, 0xfe, 0x00], false);
        assert_eq!(b, Body::Binary { old: 2, new: 3 });
    }

    #[test]
    fn a_file_with_no_trailing_newline_still_diffs_cleanly() {
        let d = unified("a\nb", "a\nc");
        assert!(d.contains("-b"), "{d}");
        assert!(d.contains("+c"), "{d}");
    }

    #[test]
    fn colourising_preserves_every_line() {
        let plain = "-old\n+new\n ctx";
        let coloured = colourise(plain);
        assert_eq!(coloured.lines().count(), 3);
        for needle in ["old", "new", "ctx"] {
            assert!(coloured.contains(needle));
        }
    }

    #[test]
    fn indenting_applies_to_every_line() {
        assert_eq!(indent("a\nb", "  "), "  a\n  b");
    }
}
