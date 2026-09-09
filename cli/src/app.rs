//! The handle every command receives: resolved paths, the UI, and lazily
//! loaded configuration.
//!
//! Config loading is deferred because `hats init`, `hats version` and
//! `hats doctor` all have to work before there is any config to load.

use anyhow::{Context, Result};

use crate::cli::GlobalOpts;
use crate::config::Config;
use crate::paths::HatsPaths;
use crate::platform::Platform;
use crate::ui::{Answers, Interactive, Prompter, Ui};

pub struct App {
    pub paths: HatsPaths,
    pub ui: Ui,
    pub allow_mismatch: bool,
}

impl App {
    pub fn new(opts: &GlobalOpts) -> Result<Self> {
        let paths = HatsPaths::resolve(opts.hats_home.as_deref())?;

        // An answers file implies non-interactive: it exists precisely so the
        // run needs no human.
        let prompter: Box<dyn Prompter> = match (&opts.answers, opts.non_interactive) {
            (Some(file), _) => Box::new(Answers::from_file(file)?),
            (None, true) => Box::new(Answers::defaults_only()),
            (None, false) => Box::new(Interactive),
        };

        // HATS_DEV is the escape hatch for working against a development clone
        // whose tag will never match the binary.
        let allow_mismatch = opts.allow_mismatch || env_flag("HATS_DEV");

        Ok(Self {
            paths,
            ui: Ui::new(prompter, opts.no_color, opts.verbose),
            allow_mismatch,
        })
    }

    pub fn platform(&self) -> Result<Platform> {
        Platform::detect()
    }

    /// Load both halves of the configuration, with a message that says which
    /// step the user has not run yet.
    pub fn config(&self) -> Result<Config> {
        Config::load(&self.paths)
            .with_context(|| format!("loading configuration from {}", self.paths.root.display()))
    }

    pub fn repo(&self) -> crate::repo::Repo<'static> {
        crate::repo::open(&self.paths.repo)
    }
}

/// An environment variable counts as set unless it is empty or an explicit
/// falsehood, so `HATS_DEV=0` does what it looks like.
fn env_flag(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no"
        ),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> GlobalOpts {
        GlobalOpts {
            hats_home: None,
            non_interactive: true,
            answers: None,
            no_color: true,
            allow_mismatch: false,
            verbose: 0,
        }
    }

    #[test]
    fn env_flag_reads_the_obvious_falsehoods_as_false() {
        // SAFETY: single-threaded test, variable is scoped to this test's name.
        unsafe {
            std::env::set_var("HATS_TEST_FLAG", "0");
            assert!(!env_flag("HATS_TEST_FLAG"));
            std::env::set_var("HATS_TEST_FLAG", "false");
            assert!(!env_flag("HATS_TEST_FLAG"));
            std::env::set_var("HATS_TEST_FLAG", "");
            assert!(!env_flag("HATS_TEST_FLAG"));
            std::env::set_var("HATS_TEST_FLAG", "1");
            assert!(env_flag("HATS_TEST_FLAG"));
            std::env::remove_var("HATS_TEST_FLAG");
        }
        assert!(!env_flag("HATS_TEST_FLAG"));
    }

    #[test]
    fn a_hats_home_override_reaches_the_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut o = opts();
        o.hats_home = Some(dir.path().to_path_buf());
        let app = App::new(&o).unwrap();
        assert_eq!(app.paths.root, dir.path());
        assert_eq!(app.paths.config, dir.path().join("config.yaml"));
    }

    #[test]
    fn non_interactive_takes_defaults_rather_than_blocking() {
        let app = App::new(&opts()).unwrap();
        let mut ui = app.ui;
        assert!(ui.prompter.confirm("k", "carry on?", true).unwrap());
        assert!(!ui.prompter.is_interactive());
    }

    #[test]
    fn config_before_init_says_to_run_init() {
        let dir = tempfile::tempdir().unwrap();
        let mut o = opts();
        o.hats_home = Some(dir.path().to_path_buf());
        let app = App::new(&o).unwrap();
        let err = app.config().unwrap_err();
        assert!(format!("{err:#}").contains("hats init"), "{err:#}");
    }
}
