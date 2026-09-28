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

        Ok(Self {
            paths,
            ui: Ui::new(prompter, opts.no_color, opts.verbose),
        })
    }

    pub fn platform(&self) -> Result<Platform> {
        Platform::detect()
    }

    /// Load the configuration, with a message that says which step the user
    /// has not run yet.
    pub fn config(&self) -> Result<Config> {
        Config::load(&self.paths)
            .with_context(|| format!("loading configuration from {}", self.paths.root.display()))
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
            verbose: 0,
        }
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
