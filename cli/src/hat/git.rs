//! Per-hat git config.
//!
//! Identity already travels in environment variables. Anything else a client
//! needs from git (a `url.insteadOf`, a `core.sshCommand`, a credential
//! helper) goes in `~/.gitconfig.d/<hat>`, which every shell wearing the hat
//! includes through git's `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` variables.
//! git skips an include that does not exist, so a hat with no file costs
//! nothing.

use std::path::{Path, PathBuf};

/// Where a hat's own git config lives.
pub fn config_path(home: &Path, hat: &str) -> PathBuf {
    home.join(".gitconfig.d").join(hat)
}

/// What `hats apply` writes into a hat's git config the one time it creates it.
pub fn scaffold_text(hat: &str) -> String {
    format!(
        "# git config for hat {hat}, read by shells wearing it. Machine-local; \
         hats created this file and will not touch it again.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hat_gets_its_own_path() {
        assert_eq!(
            config_path(Path::new("/home/t"), "acme"),
            PathBuf::from("/home/t/.gitconfig.d/acme")
        );
    }

    #[test]
    fn the_scaffold_is_a_single_comment_line() {
        let text = scaffold_text("acme");
        assert_eq!(text.lines().count(), 1);
        assert!(text.starts_with("# git config for hat acme"));
    }
}
