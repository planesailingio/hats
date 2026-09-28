//! hats — one laptop, many hats.
//!
//! A single binary for per-shell client hats: identity, cloud, kube and tool
//! isolation, switched per terminal. Machine bootstrap and dotfiles are
//! `bosun`, a sibling tool.
//!
//! The design rests on four rules, taken from the dotfiles' own docs:
//!
//! 1. Per-context state lives in environment variables, never in shared files.
//! 2. For a tool that insists on a file, give each context its own copy and
//!    point an environment variable at it.
//! 3. Unset before you set, or the last context leaks into the next.
//! 4. Put the active context in the prompt, and colour production red.
//!
//! Rule 3 used to be a hand-maintained list that drifted. Here the unset list is
//! derived from the union of every hat's keys, so it cannot.

pub mod app;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod hat;
pub mod paths;
pub mod platform;
pub mod secrets;
pub mod ui;
