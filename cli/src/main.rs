//! hats — one laptop, many hats.

use std::process::ExitCode;

use clap::Parser;

use hats::app::App;
use hats::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let mut app = match App::new(&cli.global) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("hats: {e:#}");
            return ExitCode::FAILURE;
        }
    };

    match hats::commands::dispatch(&mut app, &cli.command) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            // `{:#}` prints the whole anyhow context chain, which is where the
            // "run this instead" hints live.
            eprintln!("hats: {e:#}");
            ExitCode::FAILURE
        }
    }
}
