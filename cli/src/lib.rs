//! hats — one laptop, many hats.
//!
//! A single binary that owns a dotfiles repo and the per-shell client profiles
//! layered on top of it. It replaces chezmoi, a Makefile, six `run_` scripts and
//! a pile of hand-written zsh.
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
//! derived from the union of every profile's keys, so it cannot.

pub mod app;
pub mod cli;
pub mod commands;
pub mod config;
pub mod engine;
pub mod error;
pub mod hooks;
pub mod model;
pub mod paths;
pub mod platform;
pub mod profile;
pub mod repo;
pub mod secrets;
pub mod ui;
