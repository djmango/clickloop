//! Key names, aliases and modifiers.
//!
//! The Swift script used macOS virtual key codes as its identity. Here the
//! identity is `rdev::Key`, which each platform maps to its own code, so one
//! name table serves macOS, Windows and Linux.
//!
//! Key names are kept compatible with the Swift script, so an existing
//! `--actions "cmd+s"` keeps working, and new cross-platform spellings
//! (`win+s`, `super+s`, `f13`, `kp_plus`) are accepted alongside them.

pub use rdev::{Event, EventType, Key, listen};
use std::collections::BTreeMap;

/// A parsed key stroke: one key with the modifiers held while it is pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stroke {
    pub key: Key,
    /// Modifiers in press order, outermost first. Duplicates removed.
    pub modifiers: Vec<Modifier>,
    /// Human readable form, for the banner and confirmations.
    pub label: String,
}

/// A modifier key, tracked separately from the key it is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Meta,
    Shift,
    Alt,
    Control,
    Function,
}

impl Modifier {
    /// The `rdev::Key` that produces this modifier.
    pub fn as_key(self) -> Key {
        match self {
            Modifier::Meta => Key::MetaLeft,
            Modifier::Shift => Key::ShiftLeft,
            Modifier::Alt => Key::Alt,
            Modifier::Control => Key::ControlLeft,
            Modifier::Function => Key::Function,
        }
    }

    /// Canonical name used when rebuilding a label.
    pub fn name(self) -> &'static str {
        match self {
            Modifier::Meta => "meta",
            Modifier::Shift => "shift",
            Modifier::Alt => "alt",
            Modifier::Control => "ctrl",
            Modifier::Function => "fn",
        }
    }
}

/// A modifier name as written in a sequence, e.g. `cmd`, `opt`, `super`.
struct ModifierName {
    text: &'static str,
    modifier: Modifier,
}

/// Every spelling of a modifier accepted before a `+`.
const MODIFIER_NAMES: &[ModifierName] = &[
    ModifierName {
        text: "cmd",
        modifier: Modifier::Meta,
    },
    ModifierName {
        text: "command",
        modifier: Modifier::Meta,
    },
    ModifierName {
        text: "meta",
        modifier: Modifier::Meta,
    },
    ModifierName {
        text: "super",
        modifier: Modifier::Meta,
    },
    ModifierName {
        text: "win",
        modifier: Modifier::Meta,
    },
    ModifierName {
        text: "windows",
        modifier: Modifier::Meta,
    },
    ModifierName {
        text: "shift",
        modifier: Modifier::Shift,
    },
    ModifierName {
        text: "alt",
        modifier: Modifier::Alt,
    },
    ModifierName {
        text: "opt",
        modifier: Modifier::Alt,
    },
    ModifierName {
        text: "option",
        modifier: Modifier::Alt,
    },
    ModifierName {
        text: "ctrl",
        modifier: Modifier::Control,
    },
    ModifierName {
        text: "control",
        modifier: Modifier::Control,
    },
    ModifierName {
        text: "fn",
        modifier: Modifier::Function,
    },
];

/// Names that mean a modifier key on its own (no `+` follows).
const MODIFIER_KEY_NAMES: &[(&str, Key)] = &[
    ("cmd", Key::MetaLeft),
    ("command", Key::MetaLeft),
    ("meta", Key::MetaLeft),
    ("super", Key::MetaLeft),
    ("win", Key::MetaLeft),
    ("windows", Key::MetaLeft),
    ("shift", Key::ShiftLeft),
    ("alt", Key::Alt),
    ("opt", Key::Alt),
    ("option", Key::Alt),
    ("ctrl", Key::ControlLeft),
    ("control", Key::ControlLeft),
    ("fn", Key::Function),
];

/// Spellings that map onto a canonical key name.
const ALIASES: &[(&str, &str)] = &[
    ("esc", "escape"),
    ("enter", "return"),
    ("ret", "return"),
    ("spacebar", "space"),
    ("delete", "backspace"),
    ("del", "forwarddelete"),
    ("pgup", "pageup"),
    ("pgdn", "pagedown"),
    ("arrowup", "up"),
    ("arrowdown", "down"),
    ("arrowleft", "left"),
    ("arrowright", "right"),
    ("-", "minus"),
    ("=", "equal"),
    ("[", "lbracket"),
    ("]", "rbracket"),
    ("\\", "backslash"),
    (";", "semicolon"),
    ("'", "quote"),
    ("`", "grave"),
    (",", "comma"),
    (".", "period"),
    ("/", "slash"),
];

/// The canonical key table: every name accepted in a sequence, and its key.
///
/// Names follow the Swift script (`leftshift`, `keypad_plus`, `forwarddelete`)
/// and add the natural cross-platform spellings (`shiftleft`, `kp_plus`,
/// `insert`, `printscreen`, media keys).
fn key_table() -> BTreeMap<&'static str, Key> {
    let mut table: BTreeMap<&'static str, Key> = BTreeMap::new();

    // Letters.
    for (name, key) in [
        ("a", Key::KeyA),
        ("b", Key::KeyB),
        ("c", Key::KeyC),
        ("d", Key::KeyD),
        ("e", Key::KeyE),
        ("f", Key::KeyF),
        ("g", Key::KeyG),
        ("h", Key::KeyH),
        ("i", Key::KeyI),
        ("j", Key::KeyJ),
        ("k", Key::KeyK),
        ("l", Key::KeyL),
        ("m", Key::KeyM),
        ("n", Key::KeyN),
        ("o", Key::KeyO),
        ("p", Key::KeyP),
        ("q", Key::KeyQ),
        ("r", Key::KeyR),
        ("s", Key::KeyS),
        ("t", Key::KeyT),
        ("u", Key::KeyU),
        ("v", Key::KeyV),
        ("w", Key::KeyW),
        ("x", Key::KeyX),
        ("y", Key::KeyY),
        ("z", Key::KeyZ),
    ] {
        table.insert(name, key);
    }

    // Number row, as digits and as the `num1` spelling.
    for (name, num_name, key) in [
        ("0", "num0", Key::Num0),
        ("1", "num1", Key::Num1),
        ("2", "num2", Key::Num2),
        ("3", "num3", Key::Num3),
        ("4", "num4", Key::Num4),
        ("5", "num5", Key::Num5),
        ("6", "num6", Key::Num6),
        ("7", "num7", Key::Num7),
        ("8", "num8", Key::Num8),
        ("9", "num9", Key::Num9),
    ] {
        table.insert(name, key);
        table.insert(num_name, key);
    }

    // Function keys.
    for (name, key) in [
        ("f1", Key::F1),
        ("f2", Key::F2),
        ("f3", Key::F3),
        ("f4", Key::F4),
        ("f5", Key::F5),
        ("f6", Key::F6),
        ("f7", Key::F7),
        ("f8", Key::F8),
        ("f9", Key::F9),
        ("f10", Key::F10),
        ("f11", Key::F11),
        ("f12", Key::F12),
        ("f13", Key::F13),
        ("f14", Key::F14),
        ("f15", Key::F15),
        ("f16", Key::F16),
        ("f17", Key::F17),
        ("f18", Key::F18),
        ("f19", Key::F19),
        ("f20", Key::F20),
        ("f21", Key::F21),
        ("f22", Key::F22),
        ("f23", Key::F23),
        ("f24", Key::F24),
    ] {
        table.insert(name, key);
    }

    // Editing and navigation.
    for (name, key) in [
        ("return", Key::Return),
        ("tab", Key::Tab),
        ("space", Key::Space),
        ("backspace", Key::Backspace),
        ("forwarddelete", Key::Delete),
        ("insert", Key::Insert),
        ("escape", Key::Escape),
        ("home", Key::Home),
        ("end", Key::End),
        ("pageup", Key::PageUp),
        ("pagedown", Key::PageDown),
        ("left", Key::LeftArrow),
        ("right", Key::RightArrow),
        ("up", Key::UpArrow),
        ("down", Key::DownArrow),
        ("capslock", Key::CapsLock),
        ("numlock", Key::NumLock),
        ("scrolllock", Key::ScrollLock),
        ("pause", Key::Pause),
        ("printscreen", Key::PrintScreen),
        ("apps", Key::Apps),
        ("cancel", Key::Cancel),
        ("clear", Key::Clear),
        ("help", Key::Help),
        ("select", Key::Select),
        ("execute", Key::Execute),
        ("print", Key::Print),
        ("sleep", Key::Sleep),
        ("separator", Key::Separator),
    ] {
        table.insert(name, key);
    }

    // Punctuation, by macOS name and by symbol.
    for (name, key) in [
        ("minus", Key::Minus),
        ("equal", Key::Equal),
        ("lbracket", Key::LeftBracket),
        ("rbracket", Key::RightBracket),
        ("backslash", Key::BackSlash),
        ("intlbackslash", Key::IntlBackslash),
        ("intlro", Key::IntlRo),
        ("intlyen", Key::IntlYen),
        ("semicolon", Key::SemiColon),
        ("quote", Key::Quote),
        ("grave", Key::BackQuote),
        ("comma", Key::Comma),
        ("period", Key::Dot),
        ("dot", Key::Dot),
        ("slash", Key::Slash),
    ] {
        table.insert(name, key);
    }

    // Physical modifiers, both Swift spellings and `shiftleft` style.
    for (name, key) in [
        ("leftshift", Key::ShiftLeft),
        ("shiftleft", Key::ShiftLeft),
        ("rightshift", Key::ShiftRight),
        ("shiftright", Key::ShiftRight),
        ("leftctrl", Key::ControlLeft),
        ("ctrlleft", Key::ControlLeft),
        ("leftcontrol", Key::ControlLeft),
        ("rightctrl", Key::ControlRight),
        ("ctrlright", Key::ControlRight),
        ("rightcontrol", Key::ControlRight),
        ("leftalt", Key::Alt),
        ("altleft", Key::Alt),
        ("rightalt", Key::AltGr),
        ("altright", Key::AltGr),
        ("altgr", Key::AltGr),
        ("leftcmd", Key::MetaLeft),
        ("rightcmd", Key::MetaRight),
        ("cmdleft", Key::MetaLeft),
        ("cmdright", Key::MetaRight),
        ("metaleft", Key::MetaLeft),
        ("metaright", Key::MetaRight),
        ("superleft", Key::MetaLeft),
        ("superright", Key::MetaRight),
    ] {
        table.insert(name, key);
    }

    // Keypad. The Swift names are `keypad_*`; `kp_*` is the short form.
    for (name, key) in [
        ("keypad_decimal", Key::KpDecimal),
        ("kp_decimal", Key::KpDecimal),
        ("keypad_multiply", Key::KpMultiply),
        ("kp_multiply", Key::KpMultiply),
        ("keypad_plus", Key::KpPlus),
        ("kp_plus", Key::KpPlus),
        ("keypad_minus", Key::KpMinus),
        ("kp_minus", Key::KpMinus),
        ("keypad_divide", Key::KpDivide),
        ("kp_divide", Key::KpDivide),
        ("keypad_enter", Key::KpReturn),
        ("kp_enter", Key::KpReturn),
        ("keypad_equal", Key::KpEqual),
        ("kp_equal", Key::KpEqual),
        ("keypad_comma", Key::KpComma),
        ("kp_comma", Key::KpComma),
        ("keypad_clear", Key::Clear),
        ("kp_clear", Key::Clear),
        ("keypad_0", Key::Kp0),
        ("kp_0", Key::Kp0),
        ("keypad_1", Key::Kp1),
        ("kp_1", Key::Kp1),
        ("keypad_2", Key::Kp2),
        ("kp_2", Key::Kp2),
        ("keypad_3", Key::Kp3),
        ("kp_3", Key::Kp3),
        ("keypad_4", Key::Kp4),
        ("kp_4", Key::Kp4),
        ("keypad_5", Key::Kp5),
        ("kp_5", Key::Kp5),
        ("keypad_6", Key::Kp6),
        ("kp_6", Key::Kp6),
        ("keypad_7", Key::Kp7),
        ("kp_7", Key::Kp7),
        ("keypad_8", Key::Kp8),
        ("kp_8", Key::Kp8),
        ("keypad_9", Key::Kp9),
        ("kp_9", Key::Kp9),
    ] {
        table.insert(name, key);
    }

    // Media and launch keys.
    for (name, key) in [
        ("volumeup", Key::VolumeUp),
        ("volumedown", Key::VolumeDown),
        ("volumemute", Key::VolumeMute),
        ("medianexttrack", Key::MediaNextTrack),
        ("mediaprevtrack", Key::MediaPrevTrack),
        ("mediaplaypause", Key::MediaPlayPause),
        ("browsersearch", Key::BrowserSearch),
        ("launchmail", Key::LaunchMail),
        ("launchapp2", Key::LaunchApp2),
    ] {
        table.insert(name, key);
    }

    table
}

/// Every canonical key name, sorted. Used by `--list-keys`.
pub fn key_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = key_table().keys().copied().collect();
    // The bare modifier names are keys too, and `fn` is not in the table above,
    // so they are listed explicitly.
    for (text, _) in MODIFIER_KEY_NAMES {
        if !names.contains(text) {
            names.push(text);
        }
    }
    names.sort_unstable();
    names
}

/// Looks up one canonical name (no aliases applied) and returns its key.
pub fn key_by_name(name: &str) -> Option<Key> {
    if let Some(key) = key_table().get(name) {
        return Some(*key);
    }
    MODIFIER_KEY_NAMES
        .iter()
        .find(|(text, _)| *text == name)
        .map(|(_, key)| *key)
}

fn canonical_name(token: &str) -> &str {
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == token)
        .map(|(_, name)| *name)
        .unwrap_or(token)
}

fn modifier_by_name(token: &str) -> Option<Modifier> {
    MODIFIER_NAMES
        .iter()
        .find(|entry| entry.text == token)
        .map(|entry| entry.modifier)
}

/// Parses a key token such as `a`, `cmd+s`, `shift+space` or `kp_plus`.
///
/// A modifier on its own (`shift`) is a key press of that modifier. A
/// modifier followed by `+` and a key (`cmd+s`) becomes that key with the
/// modifier held. The last `+` segment is the key; everything before it must
/// be a modifier.
pub fn parse_stroke(raw: &str) -> Option<Stroke> {
    let token = raw.trim().to_lowercase();
    if token.is_empty() {
        return None;
    }

    // Split on '+', but keep a trailing '+' segment meaningful: "cmd++" is
    // not a supported spelling, so an empty segment anywhere is an error.
    let parts: Vec<&str> = token.split('+').map(str::trim).collect();
    if parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let (last, modifiers_raw) = parts.split_last()?;

    let mut modifiers: Vec<Modifier> = Vec::new();
    for name in modifiers_raw {
        let modifier = modifier_by_name(name)?;
        if !modifiers.contains(&modifier) {
            modifiers.push(modifier);
        }
    }

    let name = canonical_name(last);
    let key = key_by_name(name)?;

    // The label quotes modifiers the way the user wrote them and the key by its
    // canonical name, matching the Swift original: "esc" reads back as "escape"
    // while "CMD+S" reads back as "cmd+s".
    let mut labels: Vec<&str> = modifiers_raw.to_vec();
    labels.push(name);

    Some(Stroke {
        key,
        modifiers,
        label: labels.join("+"),
    })
}

/// The key press depth of a stroke: presses key, then modifiers.
impl Stroke {
    /// Order to press: modifiers first, then the key.
    pub fn press_order(&self) -> Vec<Key> {
        let mut order: Vec<Key> = self.modifiers.iter().map(|m| m.as_key()).collect();
        order.push(self.key);
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_keys() {
        assert_eq!(parse_stroke("a").unwrap().key, Key::KeyA);
        assert_eq!(parse_stroke("1").unwrap().key, Key::Num1);
        assert_eq!(parse_stroke("f12").unwrap().key, Key::F12);
        assert!(parse_stroke("A").unwrap().label == "a");
    }

    #[test]
    fn aliases() {
        assert_eq!(parse_stroke("esc").unwrap().key, Key::Escape);
        assert_eq!(parse_stroke("enter").unwrap().key, Key::Return);
        assert_eq!(parse_stroke("-").unwrap().key, Key::Minus);
        assert_eq!(parse_stroke("del").unwrap().key, Key::Delete);
    }

    #[test]
    fn label_quotes_modifiers_as_typed_and_keys_canonically() {
        // The Swift original labels "esc" as "escape" and "CMD+S" as "cmd+s":
        // modifiers keep the spelling given, keys use the canonical name.
        assert_eq!(parse_stroke("esc").unwrap().label, "escape");
        assert_eq!(parse_stroke("cmd+s").unwrap().label, "cmd+s");
        assert_eq!(parse_stroke("CMD+S").unwrap().label, "cmd+s");
        assert_eq!(parse_stroke("command+s").unwrap().label, "command+s");
        assert_eq!(parse_stroke("9").unwrap().label, "9");
    }

    #[test]
    fn chords() {
        let stroke = parse_stroke("cmd+s").unwrap();
        assert_eq!(stroke.key, Key::KeyS);
        assert_eq!(stroke.modifiers, vec![Modifier::Meta]);
        assert_eq!(stroke.label, "cmd+s");
        assert_eq!(stroke.press_order(), vec![Key::MetaLeft, Key::KeyS]);
    }

    #[test]
    fn chord_order_is_outermost_first() {
        let stroke = parse_stroke("ctrl+shift+s").unwrap();
        assert_eq!(
            stroke.press_order(),
            vec![Key::ControlLeft, Key::ShiftLeft, Key::KeyS]
        );
    }

    #[test]
    fn duplicate_modifiers_collapse() {
        let stroke = parse_stroke("cmd+command+s").unwrap();
        assert_eq!(stroke.modifiers, vec![Modifier::Meta]);
    }

    #[test]
    fn modifier_alone_is_a_key() {
        assert_eq!(parse_stroke("shift").unwrap().key, Key::ShiftLeft);
        assert!(parse_stroke("shift").unwrap().modifiers.is_empty());
    }

    #[test]
    fn rejects_bad_tokens() {
        assert!(parse_stroke("").is_none());
        assert!(parse_stroke("notakey").is_none());
        assert!(parse_stroke("cmd+").is_none());
        assert!(parse_stroke("+s").is_none());
        assert!(parse_stroke("cmd+nope").is_none());
    }

    #[test]
    fn cross_platform_modifier_spellings() {
        for name in ["cmd", "command", "meta", "super", "win", "windows"] {
            let stroke = parse_stroke(&format!("{name}+s")).unwrap();
            assert_eq!(stroke.modifiers, vec![Modifier::Meta], "{name}");
        }
    }

    #[test]
    fn every_name_parses() {
        for name in key_names() {
            assert!(parse_stroke(name).is_some(), "{name} did not parse");
        }
    }

    #[test]
    fn every_swift_key_name_is_covered() {
        // The Swift script's key table, transcribed from its --list-keys
        // output. Every one of these must parse in the port, so an existing
        // --actions keeps working after the switch.
        let swift_names = [
            "a",
            "s",
            "d",
            "f",
            "h",
            "g",
            "z",
            "x",
            "c",
            "v",
            "b",
            "q",
            "w",
            "e",
            "r",
            "y",
            "t",
            "1",
            "2",
            "3",
            "4",
            "6",
            "5",
            "equal",
            "9",
            "7",
            "minus",
            "8",
            "0",
            "rbracket",
            "o",
            "u",
            "lbracket",
            "i",
            "p",
            "return",
            "l",
            "j",
            "quote",
            "k",
            "semicolon",
            "backslash",
            "comma",
            "slash",
            "n",
            "m",
            "period",
            "tab",
            "space",
            "grave",
            "backspace",
            "escape",
            "rightcmd",
            "leftcmd",
            "leftshift",
            "capslock",
            "leftalt",
            "leftctrl",
            "rightshift",
            "rightalt",
            "rightctrl",
            "fn",
            "f17",
            "keypad_decimal",
            "keypad_multiply",
            "keypad_plus",
            "keypad_clear",
            "keypad_divide",
            "keypad_enter",
            "keypad_minus",
            "f18",
            "f19",
            "keypad_equal",
            "keypad_0",
            "keypad_1",
            "keypad_2",
            "keypad_3",
            "keypad_4",
            "keypad_5",
            "keypad_6",
            "keypad_7",
            "f20",
            "keypad_8",
            "keypad_9",
            "f5",
            "f6",
            "f7",
            "f3",
            "f8",
            "f9",
            "f11",
            "f13",
            "f16",
            "f14",
            "f10",
            "f12",
            "home",
            "pageup",
            "forwarddelete",
            "f4",
            "end",
            "f2",
            "pagedown",
            "f1",
            "left",
            "right",
            "down",
            "up",
        ];
        for name in swift_names {
            assert!(
                parse_stroke(name).is_some(),
                "Swift key name {name} is missing from the port"
            );
            assert!(
                key_names().contains(&name),
                "Swift key name {name} is missing from --list-keys"
            );
        }
    }
}
