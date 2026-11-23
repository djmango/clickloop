//! Global hotkeys: F5 starts a run and F6 stops it, whether or not the window
//! has focus.
//!
//! One passive listener runs for the life of the app. It is started once,
//! because the OS only allows a single global event tap per process, and it
//! reports presses over a channel so the window can react to them.
//!
//! The listener never swallows a key: F5 still does whatever F5 normally does
//! in the focused app.

use clickloop::keys;
use clickloop::keys::{Event, EventType, Key, listen};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Which hotkey a press belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Start,
    Stop,
}

/// Something the window needs to know about.
#[derive(Clone, Debug)]
pub enum Signal {
    /// The start hotkey was pressed.
    Start,
    /// The stop hotkey was pressed.
    Stop,
    /// A key was captured as a new binding.
    Captured(Slot, String),
}

/// The two hotkeys, as the names the user sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    pub start: String,
    pub stop: String,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            start: "f5".to_string(),
            stop: "f6".to_string(),
        }
    }
}

/// Modifier bits, so a chord like `cmd+k` can be matched.
const BIT_META: u8 = 1 << 0;
const BIT_SHIFT: u8 = 1 << 1;
const BIT_ALT: u8 = 1 << 2;
const BIT_CTRL: u8 = 1 << 3;
const BIT_FN: u8 = 1 << 4;

fn modifier_bit(key: Key) -> u8 {
    match key {
        Key::MetaLeft | Key::MetaRight => BIT_META,
        Key::ShiftLeft | Key::ShiftRight => BIT_SHIFT,
        Key::Alt | Key::AltGr => BIT_ALT,
        Key::ControlLeft | Key::ControlRight => BIT_CTRL,
        Key::Function => BIT_FN,
        _ => 0,
    }
}

fn is_modifier(key: Key) -> bool {
    modifier_bit(key) != 0
}

/// The shortest name for a key, so a captured binding reads `f5`, not `F5Key`.
pub fn key_label(key: Key) -> String {
    let mut best: Option<&'static str> = None;
    for name in keys::key_names() {
        if keys::key_by_name(name) == Some(key) {
            best = Some(match best {
                // Prefer the shortest spelling, then the alphabetically first.
                Some(current) if current.len() <= name.len() => current,
                _ => name,
            });
        }
    }
    match best {
        Some(name) => name.to_string(),
        None => format!("{key:?}").to_lowercase(),
    }
}

/// A binding as written in the window, e.g. `f5` or `ctrl+shift+s`.
fn label_for(key: Key, modifiers: u8) -> String {
    // Written order, so a label matches how people write chords: the command
    // key, then control, then alt, then shift.
    let mut parts: Vec<&str> = Vec::new();
    if modifiers & BIT_META != 0 {
        parts.push("cmd");
    }
    if modifiers & BIT_CTRL != 0 {
        parts.push("ctrl");
    }
    if modifiers & BIT_ALT != 0 {
        parts.push("alt");
    }
    if modifiers & BIT_SHIFT != 0 {
        parts.push("shift");
    }
    if modifiers & BIT_FN != 0 {
        parts.push("fn");
    }
    let name = key_label(key);
    parts.push(&name);
    parts.join("+")
}

struct Inner {
    start: Mutex<Option<keys::Stroke>>,
    stop: Mutex<Option<keys::Stroke>>,
    /// Set while the window waits for a key to bind.
    capturing: Mutex<Option<Slot>>,
    /// Keys currently held, so an auto-repeat does not fire the hotkey twice.
    down: Mutex<Vec<Key>>,
    /// Keys this app sent, so a sequence that types F5 does not start itself.
    self_keys: Mutex<Vec<(Key, Instant)>>,
    modifiers: Mutex<u8>,
    tx: Sender<Signal>,
    /// False once the listener thread gives up.
    live: Mutex<bool>,
}

/// The window's handle to the listener.
pub struct Router {
    inner: Arc<Inner>,
    rx: Receiver<Signal>,
}

impl Router {
    /// Starts the global listener. Call once, at app start.
    pub fn start(bindings: &Bindings) -> Self {
        let (tx, rx) = channel();
        let inner = Arc::new(Inner {
            start: Mutex::new(keys::parse_stroke(&bindings.start)),
            stop: Mutex::new(keys::parse_stroke(&bindings.stop)),
            capturing: Mutex::new(None),
            down: Mutex::new(Vec::new()),
            self_keys: Mutex::new(Vec::new()),
            modifiers: Mutex::new(0),
            tx,
            live: Mutex::new(true),
        });

        let handler = Arc::clone(&inner);
        let after = Arc::clone(&inner);
        std::thread::spawn(move || {
            let ok = listen(move |event| handler.on_event(&event)).is_ok();
            if !ok {
                if let Ok(mut live) = after.live.lock() {
                    *live = false;
                }
            }
        });

        Self { inner, rx }
    }

    /// A router with no listener thread, for rendering a picture of the window.
    pub fn dormant(bindings: &Bindings) -> Self {
        let (tx, rx) = channel();
        Self {
            inner: Arc::new(Inner {
                start: Mutex::new(keys::parse_stroke(&bindings.start)),
                stop: Mutex::new(keys::parse_stroke(&bindings.stop)),
                capturing: Mutex::new(None),
                down: Mutex::new(Vec::new()),
                self_keys: Mutex::new(Vec::new()),
                modifiers: Mutex::new(0),
                tx,
                live: Mutex::new(false),
            }),
            rx,
        }
    }

    /// Takes the next signal, if one is waiting. Never blocks.
    pub fn poll(&self) -> Option<Signal> {
        match self.rx.try_recv() {
            Ok(signal) => Some(signal),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    /// True once the listener thread has stopped, which on macOS means the
    /// Accessibility or Input Monitoring grant is missing.
    pub fn is_live(&self) -> bool {
        self.inner.live.lock().map(|live| *live).unwrap_or(false)
    }

    /// Asks for the next key press to become `slot`'s binding.
    pub fn capture(&self, slot: Slot) {
        if let Ok(mut capturing) = self.inner.capturing.lock() {
            *capturing = Some(slot);
        }
    }

    pub fn cancel_capture(&self) {
        if let Ok(mut capturing) = self.inner.capturing.lock() {
            *capturing = None;
        }
    }

    pub fn is_capturing(&self) -> Option<Slot> {
        self.inner.capturing.lock().ok().and_then(|slot| *slot)
    }

    pub fn set_bindings(&self, bindings: &Bindings) {
        if let Ok(mut start) = self.inner.start.lock() {
            *start = keys::parse_stroke(&bindings.start);
        }
        if let Ok(mut stop) = self.inner.stop.lock() {
            *stop = keys::parse_stroke(&bindings.stop);
        }
    }

    /// Call before sending a key, so the run cannot trigger its own hotkey.
    pub fn note_self_sent(&self, key: Key) {
        if let Ok(mut sent) = self.inner.self_keys.lock() {
            sent.push((key, Instant::now()));
            if sent.len() > 64 {
                sent.remove(0);
            }
        }
    }
}

impl Inner {
    fn held(&self) -> u8 {
        self.modifiers.lock().map(|m| *m).unwrap_or(0)
    }

    fn was_self_sent(&self, key: Key) -> bool {
        let Ok(mut sent) = self.self_keys.lock() else {
            return false;
        };
        let now = Instant::now();
        sent.retain(|(_, at)| now.duration_since(*at) < Duration::from_millis(250));
        sent.iter().any(|(sent, _)| *sent == key)
    }

    fn matches(stroke: &Option<keys::Stroke>, key: Key, held: u8) -> bool {
        match stroke {
            Some(stroke) => {
                stroke.key == key
                    && held
                        == stroke.modifiers.iter().fold(0u8, |bits, modifier| {
                            bits | match modifier {
                                keys::Modifier::Meta => BIT_META,
                                keys::Modifier::Shift => BIT_SHIFT,
                                keys::Modifier::Alt => BIT_ALT,
                                keys::Modifier::Control => BIT_CTRL,
                                keys::Modifier::Function => BIT_FN,
                            }
                        })
            }
            None => false,
        }
    }

    fn on_event(&self, event: &Event) {
        // Keep the modifier state current on both edges.
        if let EventType::KeyPress(key) | EventType::KeyRelease(key) = event.event_type {
            let bit = modifier_bit(key);
            if bit != 0 {
                if let Ok(mut held) = self.modifiers.lock() {
                    let pressed = matches!(event.event_type, EventType::KeyPress(_));
                    if pressed {
                        *held |= bit;
                    } else {
                        *held &= !bit;
                    }
                }
            }
        }

        let key = match event.event_type {
            EventType::KeyPress(key) => key,
            EventType::KeyRelease(key) => {
                if let Ok(mut down) = self.down.lock() {
                    down.retain(|held| *held != key);
                }
                return;
            }
            _ => return,
        };

        // A key this app just sent is not a user press.
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if event.is_synthetic {
            return;
        }
        if self.was_self_sent(key) {
            return;
        }

        // Held keys auto-repeat; act on the first press only.
        if let Ok(mut down) = self.down.lock() {
            if down.contains(&key) {
                return;
            }
            down.push(key);
        }

        // While capturing, the next real key becomes the binding.
        if let Ok(mut guard) = self.capturing.lock() {
            if let Some(bound) = *guard {
                if !is_modifier(key) {
                    let label = label_for(key, self.held());
                    *guard = None;
                    if let Some(stroke) = keys::parse_stroke(&label) {
                        match bound {
                            Slot::Start => {
                                if let Ok(mut start) = self.start.lock() {
                                    *start = Some(stroke);
                                }
                            }
                            Slot::Stop => {
                                if let Ok(mut stop) = self.stop.lock() {
                                    *stop = Some(stroke);
                                }
                            }
                        }
                    }
                    let _ = self.tx.send(Signal::Captured(bound, label));
                }
                return;
            }
        }

        let start = self.start.lock().ok().and_then(|stroke| stroke.clone());
        let stop = self.stop.lock().ok().and_then(|stroke| stroke.clone());
        let held = self.held();

        if Self::matches(&stop, key, held) {
            let _ = self.tx.send(Signal::Stop);
        } else if Self::matches(&start, key, held) {
            let _ = self.tx.send(Signal::Start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_are_f5_and_f6() {
        let bindings = Bindings::default();
        assert_eq!(bindings.start, "f5");
        assert_eq!(bindings.stop, "f6");
        assert!(keys::parse_stroke(&bindings.start).is_some());
        assert!(keys::parse_stroke(&bindings.stop).is_some());
    }

    #[test]
    fn labels_are_short_names() {
        assert_eq!(key_label(Key::F5), "f5");
        assert_eq!(key_label(Key::F12), "f12");
        assert_eq!(key_label(Key::KeyA), "a");
        assert_eq!(key_label(Key::Escape), "escape");
    }

    #[test]
    fn chords_label_in_a_stable_order() {
        assert_eq!(label_for(Key::KeyS, BIT_CTRL | BIT_SHIFT), "ctrl+shift+s");
        assert_eq!(label_for(Key::KeyK, BIT_META), "cmd+k");
        assert_eq!(label_for(Key::F5, 0), "f5");
    }

    #[test]
    fn modifier_keys_are_not_bindable_alone() {
        assert!(is_modifier(Key::ShiftLeft));
        assert!(is_modifier(Key::MetaRight));
        assert!(!is_modifier(Key::F5));
        assert!(!is_modifier(Key::KeyA));
    }

    #[test]
    fn a_chord_only_matches_with_the_same_modifiers() {
        let stroke = keys::parse_stroke("ctrl+s");
        assert!(Inner::matches(&stroke, Key::KeyS, BIT_CTRL));
        assert!(!Inner::matches(&stroke, Key::KeyS, 0));
        assert!(!Inner::matches(&stroke, Key::KeyS, BIT_CTRL | BIT_SHIFT));
    }

    #[test]
    fn captured_labels_round_trip() {
        // Whatever capture produces must parse, or a rebind silently fails.
        for key in [Key::F5, Key::F6, Key::KeyA, Key::Space, Key::Escape] {
            let label = label_for(key, 0);
            assert!(
                keys::parse_stroke(&label).is_some(),
                "{label} did not parse back"
            );
        }
    }
}
