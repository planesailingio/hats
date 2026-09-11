//! Per-hat ssh config.
//!
//! Nothing is exported for ssh: the managed `~/.ssh/config` includes
//! `~/.ssh/config.d/${HATS_HAT}.conf` and ssh expands the variable itself.
//! These are the paths that include reads, so `hats apply` can scaffold them.

use std::path::{Path, PathBuf};

fn dir(home: &Path) -> PathBuf {
    home.join(".ssh").join("config.d")
}

/// The managed skeleton that includes the files below.
pub fn skeleton_path(home: &Path) -> PathBuf {
    home.join(".ssh").join("config")
}

/// Where a hat's own ssh hosts and keys live.
pub fn config_path(home: &Path, hat: &str) -> PathBuf {
    dir(home).join(format!("{hat}.conf"))
}

/// Hosts every hat shares, read after the hat's own file.
pub fn common_path(home: &Path) -> PathBuf {
    dir(home).join("common.conf")
}

/// What `hats apply` writes into a hat's file (or, with no hat, common.conf)
/// the one time it creates it.
pub fn scaffold_text(hat: Option<&str>) -> String {
    let what = match hat {
        Some(hat) => format!("ssh hosts and keys for hat {hat}"),
        None => "ssh hosts every hat shares".to_string(),
    };
    format!(
        "# {what}. Machine-local; hats created this file and will not touch it \
         again. Recipe: ~/.ssh/config.d/README\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_match_the_includes_in_the_skeleton() {
        let home = Path::new("/home/t");
        assert_eq!(
            config_path(home, "acme"),
            PathBuf::from("/home/t/.ssh/config.d/acme.conf")
        );
        assert_eq!(
            common_path(home),
            PathBuf::from("/home/t/.ssh/config.d/common.conf")
        );
    }

    #[test]
    fn the_scaffold_is_a_single_comment_line() {
        for text in [scaffold_text(Some("acme")), scaffold_text(None)] {
            assert_eq!(text.lines().count(), 1, "{text}");
            assert!(text.starts_with("# ssh hosts"), "{text}");
        }
        assert!(scaffold_text(Some("acme")).contains("hat acme"));
    }
}
