//! clickloop: repeat a sequence of key presses and mouse clicks.
//!
//! A cross-platform port of `auto-click.swift`. The sequence language is the
//! same, and one command runs unchanged on macOS, Windows and Linux.

fn main() {
    std::process::exit(clickloop::run());
}
