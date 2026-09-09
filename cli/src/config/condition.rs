//! OS/arch gates shared by managed files and hooks.
//!
//! Replaces the `{{ if eq .chezmoi.os "darwin" }}` wrappers that used to be
//! inside the scripts themselves, so a Linux run simply does not schedule the
//! Dock hook rather than running it and exiting early.

use serde::{Deserialize, Serialize};

use crate::platform::{Arch, Os, Platform};

/// Restricts a file or hook to some platforms. An empty condition matches
/// everything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub os: Vec<Os>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arch: Vec<Arch>,
}

impl Condition {
    pub fn matches(&self, platform: &Platform) -> bool {
        (self.os.is_empty() || self.os.contains(&platform.os))
            && (self.arch.is_empty() || self.arch.contains(&platform.arch))
    }

    pub fn is_empty(&self) -> bool {
        self.os.is_empty() && self.arch.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn platform(os: Os, arch: Arch) -> Platform {
        Platform {
            os,
            arch,
            home: PathBuf::from("/home/t"),
            user: "t".into(),
            hostname: "h".into(),
            brew_prefix: Platform::brew_prefix_for(os, arch),
        }
    }

    #[test]
    fn an_empty_condition_matches_everything() {
        let c = Condition::default();
        assert!(c.is_empty());
        assert!(c.matches(&platform(Os::Darwin, Arch::Arm64)));
        assert!(c.matches(&platform(Os::Linux, Arch::Amd64)));
    }

    #[test]
    fn os_gates_the_dock_hook_off_linux() {
        let c = Condition {
            os: vec![Os::Darwin],
            arch: vec![],
        };
        assert!(c.matches(&platform(Os::Darwin, Arch::Arm64)));
        assert!(!c.matches(&platform(Os::Linux, Arch::Amd64)));
    }

    #[test]
    fn os_and_arch_must_both_match() {
        let c = Condition {
            os: vec![Os::Darwin],
            arch: vec![Arch::Arm64],
        };
        assert!(c.matches(&platform(Os::Darwin, Arch::Arm64)));
        assert!(!c.matches(&platform(Os::Darwin, Arch::Amd64)));
    }

    #[test]
    fn deserialises_from_the_manifest_shape() {
        let c: Condition = serde_yaml_ng::from_str("os: [darwin]\n").unwrap();
        assert_eq!(c.os, vec![Os::Darwin]);
    }
}
