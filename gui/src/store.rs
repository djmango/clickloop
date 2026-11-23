//! Settings and run history on disk, so a restart keeps your hotkeys and the
//! sequences you have used.
//!
//! Plain text, one entry per line, in the platform's usual config folder:
//! `~/Library/Application Support/clickloop` on macOS, `%APPDATA%\clickloop` on
//! Windows, `$XDG_CONFIG_HOME/clickloop` (or `~/.config/clickloop`) elsewhere.

use crate::hotkey::Bindings;
use std::fs;
use std::path::PathBuf;

/// How many past sequences to keep.
pub const HISTORY_LIMIT: usize = 20;

fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/Application Support/clickloop"))
    }
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(app_data).join("clickloop"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(base).join("clickloop"));
        }
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".config/clickloop"))
    }
}

fn history_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("history.txt"))
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("settings.txt"))
}

/// Past sequences, newest first.
pub fn load_history() -> Vec<String> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(HISTORY_LIMIT)
        .map(str::to_string)
        .collect()
}

/// Adds `sequence` to the front, removing an earlier copy, and saves.
pub fn remember(sequence: &str) -> Vec<String> {
    let sequence = sequence.trim();
    let mut items = load_history();
    items.retain(|existing| existing != sequence);
    if !sequence.is_empty() {
        items.insert(0, sequence.to_string());
    }
    items.truncate(HISTORY_LIMIT);
    save_history(&items);
    items
}

fn save_history(items: &[String]) {
    let Some(path) = history_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let body: String = items.iter().map(|line| format!("{line}\n")).collect();
    let _ = fs::write(path, body);
}

pub fn load_bindings() -> Bindings {
    let defaults = Bindings::default();
    let Some(path) = settings_path() else {
        return defaults;
    };
    let Ok(text) = fs::read_to_string(path) else {
        return defaults;
    };
    let mut bindings = defaults;
    for line in text.lines() {
        match line.split_once('=') {
            Some(("start", value)) if !value.trim().is_empty() => {
                bindings.start = value.trim().to_string();
            }
            Some(("stop", value)) if !value.trim().is_empty() => {
                bindings.stop = value.trim().to_string();
            }
            _ => {}
        }
    }
    // A binding that cannot be parsed would leave the hotkey dead, so fall back.
    if clickloop::keys::parse_stroke(&bindings.start).is_none() {
        bindings.start = Bindings::default().start;
    }
    if clickloop::keys::parse_stroke(&bindings.stop).is_none() {
        bindings.stop = Bindings::default().stop;
    }
    bindings
}

pub fn save_bindings(bindings: &Bindings) {
    let Some(path) = settings_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let body = format!("start={}\nstop={}\n", bindings.start, bindings.stop);
    let _ = fs::write(path, body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_dedupes_and_keeps_newest_first() {
        let mut items = vec!["a".to_string(), "b".to_string()];
        // Simulate remember's ordering rule without touching disk.
        let sequence = "b";
        items.retain(|existing| existing != sequence);
        items.insert(0, sequence.to_string());
        assert_eq!(items, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn config_dir_resolves() {
        assert!(
            config_dir().is_some(),
            "no config directory on this platform"
        );
    }
}
