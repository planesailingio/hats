//! Facts about the machine that templates and hooks branch on.
//!
//! This replaces chezmoi's `.chezmoi.os` / `.chezmoi.arch` plus the five-line
//! Homebrew-prefix dance that was copy-pasted into every template: the prefix
//! is computed once here and exposed as `brew_prefix`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Operating systems hats knows about. Anything else is a hard error rather
/// than a silent wrong branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Darwin,
    Linux,
}

impl Os {
    pub fn as_str(self) -> &'static str {
        match self {
            Os::Darwin => "darwin",
            Os::Linux => "linux",
        }
    }

    pub fn detect() -> Result<Self> {
        match std::env::consts::OS {
            "macos" => Ok(Os::Darwin),
            "linux" => Ok(Os::Linux),
            other => anyhow::bail!("hats does not support this operating system: {other}"),
        }
    }
}

impl std::fmt::Display for Os {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// CPU architectures, normalised to the strings the old chezmoi templates used
/// (`arm64`, `amd64`) so the migrated templates read the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    Arm64,
    Amd64,
}

impl Arch {
    pub fn as_str(self) -> &'static str {
        match self {
            Arch::Arm64 => "arm64",
            Arch::Amd64 => "amd64",
        }
    }

    pub fn detect() -> Result<Self> {
        match std::env::consts::ARCH {
            "aarch64" => Ok(Arch::Arm64),
            "x86_64" => Ok(Arch::Amd64),
            other => anyhow::bail!("hats does not support this architecture: {other}"),
        }
    }
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The machine hats is running on.
#[derive(Debug, Clone, Serialize)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
    pub home: PathBuf,
    pub user: String,
    pub hostname: String,
    /// Homebrew prefix for this (os, arch): the single source of truth that
    /// used to be repeated in five templates.
    pub brew_prefix: PathBuf,
}

impl Platform {
    pub fn detect() -> Result<Self> {
        let os = Os::detect()?;
        let arch = Arch::detect()?;
        let home = directories::UserDirs::new()
            .context("could not determine the home directory (is $HOME set?)")?
            .home_dir()
            .to_path_buf();
        Ok(Self {
            brew_prefix: Self::brew_prefix_for(os, arch),
            os,
            arch,
            home,
            user: std::env::var("USER").unwrap_or_else(|_| "unknown".into()),
            hostname: hostname(),
        })
    }

    /// Apple Silicon uses /opt/homebrew, Intel macOS /usr/local, Linux the
    /// linuxbrew path. Pure function so it can be tested off-platform.
    pub fn brew_prefix_for(os: Os, arch: Arch) -> PathBuf {
        match (os, arch) {
            (Os::Darwin, Arch::Arm64) => PathBuf::from("/opt/homebrew"),
            (Os::Darwin, Arch::Amd64) => PathBuf::from("/usr/local"),
            (Os::Linux, _) => PathBuf::from("/home/linuxbrew/.linuxbrew"),
        }
    }
}

fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brew_prefix_matches_the_old_template_logic() {
        assert_eq!(
            Platform::brew_prefix_for(Os::Darwin, Arch::Arm64),
            PathBuf::from("/opt/homebrew")
        );
        assert_eq!(
            Platform::brew_prefix_for(Os::Darwin, Arch::Amd64),
            PathBuf::from("/usr/local")
        );
        assert_eq!(
            Platform::brew_prefix_for(Os::Linux, Arch::Amd64),
            PathBuf::from("/home/linuxbrew/.linuxbrew")
        );
        assert_eq!(
            Platform::brew_prefix_for(Os::Linux, Arch::Arm64),
            PathBuf::from("/home/linuxbrew/.linuxbrew")
        );
    }

    #[test]
    fn os_and_arch_render_as_the_template_strings() {
        assert_eq!(Os::Darwin.to_string(), "darwin");
        assert_eq!(Arch::Arm64.to_string(), "arm64");
        assert_eq!(Arch::Amd64.to_string(), "amd64");
    }

    #[test]
    fn detects_this_machine() {
        let p = Platform::detect().expect("tests run on macOS or Linux");
        assert!(p.home.is_absolute());
    }
}
