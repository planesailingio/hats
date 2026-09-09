//! Template rendering, and the context every template sees.
//!
//! Replaces chezmoi's Go templates with minijinja. The context is assembled in
//! exactly one place, [`RenderContext::build`], so `plan`, `apply`, `render`
//! and `lint` cannot disagree about what a template can reference.
//!
//! The five-line Homebrew-prefix dance that used to be copy-pasted into every
//! template is now a single `{{ brew_prefix }}`.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use minijinja::{Environment, Value, context};
use serde::Serialize;

use crate::config::Config;
use crate::hat::emit::sh_quote;
use crate::model::ManagedFile;
use crate::platform::Platform;
use crate::secrets::store::{Secrets, placeholder};

/// Whether templates see real secret values or placeholders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactMode {
    /// Real values, for an apply.
    Real,
    /// `«secret:name»`, for linting and for rendering a file nobody will keep.
    Placeholder,
}

/// The variables available to every template.
#[derive(Debug, Serialize)]
pub struct RenderContext {
    pub os: String,
    pub arch: String,
    pub home: String,
    pub user: String,
    pub hostname: String,
    /// `/opt/homebrew`, `/usr/local` or the linuxbrew path, decided once.
    pub brew_prefix: String,
    /// The dotfiles checkout.
    pub repo_dir: String,
    /// The `files/` directory inside it.
    pub files_dir: String,
    pub version: String,
    pub identity: IdentityView,
    pub secrets: BTreeMap<String, String>,
    pub hat_names: Vec<String>,
    pub default_hat: String,
    pub machine: BTreeMap<String, String>,
}

#[derive(Debug, Default, Serialize)]
pub struct IdentityView {
    pub name: String,
    pub email: String,
}

impl RenderContext {
    pub fn build(
        cfg: &Config,
        platform: &Platform,
        secrets: &Secrets,
        repo_dir: &Path,
        files_dir: &Path,
        mode: RedactMode,
    ) -> Self {
        // Every key any hat refers to is present, so `{{ secrets.x }}` is
        // never an undefined-variable error: an unfetched secret renders empty,
        // and the plan header says how many are missing.
        let mut keys: std::collections::BTreeSet<String> =
            secrets.keys().iter().map(|s| (*s).to_string()).collect();
        keys.extend(cfg.repo.secrets.required.iter().cloned());
        for name in cfg.hats().keys() {
            if let Ok(p) = cfg.resolve_hat(name) {
                keys.extend(p.secret_refs());
            }
        }

        let values = keys
            .into_iter()
            .map(|k| {
                let v = match mode {
                    RedactMode::Real => secrets.value_or_empty(&k),
                    RedactMode::Placeholder => {
                        if secrets.get(&k).is_some() {
                            placeholder(&k)
                        } else {
                            String::new()
                        }
                    }
                };
                (k, v)
            })
            .collect();

        let identity = cfg
            .local
            .identity
            .as_ref()
            .map(|i| IdentityView {
                name: i.name.clone().unwrap_or_default(),
                email: i.email.clone().unwrap_or_default(),
            })
            .unwrap_or_default();

        Self {
            os: platform.os.to_string(),
            arch: platform.arch.to_string(),
            home: platform.home.to_string_lossy().into_owned(),
            user: platform.user.clone(),
            hostname: platform.hostname.clone(),
            brew_prefix: platform.brew_prefix.to_string_lossy().into_owned(),
            repo_dir: repo_dir.to_string_lossy().into_owned(),
            files_dir: files_dir.to_string_lossy().into_owned(),
            version: crate::repo::BINARY_VERSION.to_string(),
            identity,
            secrets: values,
            hat_names: cfg.hat_names(),
            default_hat: cfg.local.default_hat(),
            machine: cfg.local.machine.clone().into_iter().collect(),
        }
    }

    fn to_value(&self) -> Value {
        context! {
            os => self.os,
            arch => self.arch,
            home => self.home,
            user => self.user,
            hostname => self.hostname,
            brew_prefix => self.brew_prefix,
            repo_dir => self.repo_dir,
            files_dir => self.files_dir,
            version => self.version,
            identity => &self.identity,
            secrets => &self.secrets,
            hat_names => &self.hat_names,
            default_hat => self.default_hat,
            machine => &self.machine,
        }
    }
}

/// Renders managed files.
pub struct Renderer<'a> {
    env: Environment<'a>,
    ctx: Value,
}

impl<'a> Renderer<'a> {
    pub fn new(ctx: &RenderContext) -> Self {
        let mut env = Environment::new();
        // Match Go templates' whitespace behaviour closely enough that the
        // migrated files need no `-` markers on ordinary block tags.
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);
        // Jinja drops a template's final newline by default. For dotfiles that
        // is silent corruption: shell rc files, ssh configs and Brewfiles are
        // all expected to end with one.
        env.set_keep_trailing_newline(true);
        // Referencing a variable that does not exist is a bug in the template,
        // not something to paper over with an empty string.
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.add_filter("sh_quote", |v: String| sh_quote(&v));
        Self {
            env,
            ctx: ctx.to_value(),
        }
    }

    /// Render one managed file. Non-templates are read verbatim, which is what
    /// keeps binary theme files intact.
    pub fn render(&self, file: &ManagedFile) -> Result<Vec<u8>> {
        let raw = std::fs::read(&file.source)
            .with_context(|| format!("reading {}", file.source.display()))?;
        if !file.template {
            return Ok(raw);
        }
        let text = String::from_utf8(raw).with_context(|| {
            format!(
                "{} is a template but not valid UTF-8",
                file.source.display()
            )
        })?;
        self.render_str(&text, &file.source.display().to_string())
            .map(String::into_bytes)
    }

    /// Render a template string. The name is only used in error messages.
    ///
    /// `template_from_str` compiles without registering the source in the
    /// environment, so rendering many files needs neither a growing template
    /// map nor a leaked source string.
    pub fn render_str(&self, text: &str, name: &str) -> Result<String> {
        self.env
            .template_from_str(text)
            .with_context(|| format!("parsing template {name}"))?
            .render(&self.ctx)
            .with_context(|| format!("rendering template {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::local::LocalConfig;
    use crate::config::repo::RepoConfig;
    use crate::platform::{Arch, Os};

    fn platform(os: Os, arch: Arch) -> Platform {
        Platform {
            os,
            arch,
            home: std::path::PathBuf::from("/home/t"),
            user: "t".into(),
            hostname: "mbp".into(),
            brew_prefix: Platform::brew_prefix_for(os, arch),
        }
    }

    fn config() -> Config {
        Config {
            repo: serde_yaml_ng::from_str::<RepoConfig>(
                "groups: {}\nsecrets:\n  required: [git_signing_key]\n",
            )
            .unwrap(),
            local: serde_yaml_ng::from_str::<LocalConfig>(
                "identity: { name: Jane, email: jane@example.com }\n\
                 machine: { hostname_alias: mbp }\n\
                 hats:\n  normal: {}\n  work:\n    env:\n      TOK: { secret: jira_token }\n",
            )
            .unwrap(),
        }
    }

    fn secrets() -> Secrets {
        Secrets::new(
            [
                (
                    "git_signing_key".to_string(),
                    secrecy::SecretString::from("SIGNKEY"),
                ),
                (
                    "jira_token".to_string(),
                    secrecy::SecretString::from("tok-123"),
                ),
            ]
            .into_iter()
            .collect(),
            None,
        )
    }

    fn render(template: &str, os: Os, arch: Arch, mode: RedactMode) -> String {
        let ctx = RenderContext::build(
            &config(),
            &platform(os, arch),
            &secrets(),
            Path::new("/repo"),
            Path::new("/repo/files"),
            mode,
        );
        Renderer::new(&ctx).render_str(template, "test").unwrap()
    }

    #[test]
    fn the_brew_prefix_replaces_the_old_five_line_dance() {
        assert_eq!(
            render(
                "{{ brew_prefix }}",
                Os::Darwin,
                Arch::Arm64,
                RedactMode::Real
            ),
            "/opt/homebrew"
        );
        assert_eq!(
            render(
                "{{ brew_prefix }}",
                Os::Darwin,
                Arch::Amd64,
                RedactMode::Real
            ),
            "/usr/local"
        );
        assert_eq!(
            render(
                "{{ brew_prefix }}",
                Os::Linux,
                Arch::Amd64,
                RedactMode::Real
            ),
            "/home/linuxbrew/.linuxbrew"
        );
    }

    #[test]
    fn os_conditionals_work_the_way_the_chezmoi_ones_did() {
        let t = "{% if os == \"darwin\" %}mac{% else %}other{% endif %}";
        assert_eq!(render(t, Os::Darwin, Arch::Arm64, RedactMode::Real), "mac");
        assert_eq!(render(t, Os::Linux, Arch::Amd64, RedactMode::Real), "other");
    }

    #[test]
    fn identity_and_machine_values_are_available() {
        assert_eq!(
            render(
                "{{ identity.email }}",
                Os::Darwin,
                Arch::Arm64,
                RedactMode::Real
            ),
            "jane@example.com"
        );
        assert_eq!(
            render(
                "{{ machine.hostname_alias }}",
                Os::Darwin,
                Arch::Arm64,
                RedactMode::Real
            ),
            "mbp"
        );
    }

    #[test]
    fn secrets_render_real_values_for_an_apply() {
        assert_eq!(
            render(
                "{{ secrets.jira_token }}",
                Os::Darwin,
                Arch::Arm64,
                RedactMode::Real
            ),
            "tok-123"
        );
    }

    #[test]
    fn secrets_render_placeholders_when_asked() {
        assert_eq!(
            render(
                "{{ secrets.jira_token }}",
                Os::Darwin,
                Arch::Arm64,
                RedactMode::Placeholder
            ),
            "«secret:jira_token»"
        );
    }

    /// A key referenced by a hat but never fetched must still resolve, or
    /// every template would break the moment a secret is added.
    #[test]
    fn an_unfetched_secret_renders_empty_rather_than_failing() {
        let cfg = config();
        let ctx = RenderContext::build(
            &cfg,
            &platform(Os::Darwin, Arch::Arm64),
            &Secrets::default(),
            Path::new("/repo"),
            Path::new("/repo/files"),
            RedactMode::Real,
        );
        let out = Renderer::new(&ctx)
            .render_str("[{{ secrets.jira_token }}]", "t")
            .unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn a_typo_in_a_variable_name_is_an_error_not_a_blank() {
        let ctx = RenderContext::build(
            &config(),
            &platform(Os::Darwin, Arch::Arm64),
            &secrets(),
            Path::new("/repo"),
            Path::new("/repo/files"),
            RedactMode::Real,
        );
        assert!(
            Renderer::new(&ctx)
                .render_str("{{ brew_prefixx }}", "t")
                .is_err()
        );
    }

    #[test]
    fn the_sh_quote_filter_is_available_to_templates() {
        assert_eq!(
            render(
                "{{ \"code --wait\" | sh_quote }}",
                Os::Darwin,
                Arch::Arm64,
                RedactMode::Real
            ),
            "'code --wait'"
        );
    }

    #[test]
    fn block_tags_do_not_leave_blank_lines_behind() {
        let t = "a\n{% if os == \"darwin\" %}\nb\n{% endif %}\nc\n";
        assert_eq!(
            render(t, Os::Darwin, Arch::Arm64, RedactMode::Real),
            "a\nb\nc\n"
        );
    }

    #[test]
    fn a_non_template_file_is_copied_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("theme.tmTheme");
        let bytes = [0x00u8, 0xff, 0x7b, 0x7d];
        std::fs::write(&src, bytes).unwrap();

        let file = ManagedFile {
            source: src,
            rel: "theme".into(),
            target: "/home/t/theme".into(),
            mode: None,
            dir_mode: None,
            template: false,
            secret: false,
            group: "theme".into(),
        };
        let ctx = RenderContext::build(
            &config(),
            &platform(Os::Darwin, Arch::Arm64),
            &secrets(),
            Path::new("/repo"),
            Path::new("/repo/files"),
            RedactMode::Real,
        );
        assert_eq!(Renderer::new(&ctx).render(&file).unwrap(), bytes.to_vec());
    }

    #[test]
    fn rendering_the_same_name_twice_does_not_collide() {
        let ctx = RenderContext::build(
            &config(),
            &platform(Os::Darwin, Arch::Arm64),
            &secrets(),
            Path::new("/repo"),
            Path::new("/repo/files"),
            RedactMode::Real,
        );
        let r = Renderer::new(&ctx);
        assert_eq!(r.render_str("{{ os }}", "same").unwrap(), "darwin");
        assert_eq!(r.render_str("{{ arch }}", "same").unwrap(), "arm64");
    }
}
