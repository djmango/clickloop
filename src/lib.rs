//! clickloop - repeat a sequence of key presses and mouse clicks.
//!
//! A cross-platform port of the macOS `auto-click.swift` script. The sequence
//! expression language is unchanged; the only platform specific part lives in
//! [`platform`], which drives the `rdev` crate.

pub mod cli;
pub mod control;
pub mod keys;
pub mod platform;
pub mod sequence;

pub use control::{Control, TriggerState};
pub use keys::{key_names, parse_stroke, Stroke};
pub use sequence::{describe, parse, Action};

/// Runs the command line tool and returns the process exit code.
pub fn run() -> i32 {
    cli::run()
}
