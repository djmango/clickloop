//! The platform layer: sending input and listening for the stop key.
//!
//! Everything OS specific goes through the `rdev` crate, which maps a single
//! [`rdev::Key`] onto the macOS virtual key code, the Windows scan code and the
//! Linux keysym. That is what lets one key table serve all three systems.

use crate::control::{Control, TriggerState};
use crate::keys::{Modifier, Stroke};
use crate::sequence::{Action, ClickButton};
use rdev::{Button, Event, EventType, Key};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Settle time after each posted event. macOS drops a key press that follows
/// its release too closely, and Windows needs the same courtesy between a
/// mouse down and up. A few milliseconds is invisible to a human but makes the
/// difference between "works" and "sometimes misses a click".
const SETTLE: Duration = Duration::from_millis(3);

/// Modifier bits, so the listener can track what is held down.
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

fn modifier_bits(modifiers: &[Modifier]) -> u8 {
    let mut bits = 0u8;
    for modifier in modifiers {
        bits |= match modifier {
            Modifier::Meta => BIT_META,
            Modifier::Shift => BIT_SHIFT,
            Modifier::Alt => BIT_ALT,
            Modifier::Control => BIT_CTRL,
            Modifier::Function => BIT_FN,
        };
    }
    bits
}

/// Sends one action. Returns false when the OS refused the event.
///
/// Waits are not sent: the run loop handles those, because only it knows the
/// stop flag and the time limit.
pub fn send_action(action: &Action) -> bool {
    match action {
        Action::Key(stroke) => send_stroke(stroke),
        Action::Click(button, _) => send_click(*button),
        Action::Wait(_) => true,
    }
}

/// Presses modifiers, taps the key, releases the modifiers in reverse.
///
/// Chords are built from explicit key events rather than a flags field,
/// because a flags field is a macOS-only concept: Windows and Linux need the
/// modifier keys held for real, and macOS needs them too since `rdev` derives
/// the event flags from the modifier keys the listener has seen.
pub fn send_stroke(stroke: &Stroke) -> bool {
    let ordered: Vec<Key> = stroke.modifiers.iter().map(|m| m.as_key()).collect();

    for modifier in &ordered {
        if !send(&EventType::KeyPress(*modifier)) {
            return false;
        }
    }
    let pressed = send(&EventType::KeyPress(stroke.key));
    std::thread::sleep(SETTLE);
    let released = send(&EventType::KeyRelease(stroke.key));
    for modifier in ordered.iter().rev() {
        if !send(&EventType::KeyRelease(*modifier)) {
            return false;
        }
    }
    pressed && released
}

/// Clicks a mouse button where the pointer is.
pub fn send_click(button: ClickButton) -> bool {
    let button = match button {
        ClickButton::Left => Button::Left,
        ClickButton::Right => Button::Right,
    };
    if !send(&EventType::ButtonPress(button)) {
        return false;
    }
    std::thread::sleep(SETTLE);
    send(&EventType::ButtonRelease(button))
}

fn send(event: &EventType) -> bool {
    let sent = rdev::simulate(event).is_ok();
    std::thread::sleep(SETTLE);
    sent
}

/// macOS reports a missing Accessibility or Input Monitoring grant only by
/// silently delivering nothing, so it is checked up front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Input can be sent and listened for.
    Granted,
    /// Not granted: the message says what to switch on.
    Missing(&'static str),
}

#[cfg(target_os = "macos")]
mod macos_trust {
    // The same query `rdev` makes internally; kept here because the crate does
    // not re-export it.
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }
}

/// Checks that this process may send input, and listen for it, on this OS.
pub fn check_permissions() -> Permission {
    #[cfg(target_os = "macos")]
    {
        if !macos_trust::is_trusted() {
            return Permission::Missing(
                "Enable Accessibility for your terminal in System Settings > \
                 Privacy & Security > Accessibility, then run again.",
            );
        }
    }
    Permission::Granted
}

/// What the listener should do with an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signal {
    Stop,
    Trigger,
    Nothing,
}

/// State the listener thread needs.
pub struct Listener {
    control: Arc<Control>,
    trigger_state: Arc<TriggerState>,
    stop_key: Option<Stroke>,
    trigger_key: Option<Stroke>,
    modifiers: AtomicU8,
    /// Keys we sent ourselves, with the moment we sent them. Linux has no way
    /// to tell a synthetic event from a real one, so this is how a key in a
    /// sequence does not trip the stop key by mistake.
    self_keys: Mutex<Vec<(Key, Instant)>>,
}

impl Listener {
    pub fn new(
        control: Arc<Control>,
        trigger_state: Arc<TriggerState>,
        stop_key: Option<Stroke>,
        trigger_key: Option<Stroke>,
    ) -> Self {
        Self {
            control,
            trigger_state,
            stop_key,
            trigger_key,
            modifiers: AtomicU8::new(0),
            self_keys: Mutex::new(Vec::new()),
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn was_self_sent(&self, key: Key) -> bool {
        let mut keys = match self.self_keys.lock() {
            Ok(keys) => keys,
            Err(poisoned) => poisoned.into_inner(),
        };
        let now = Instant::now();
        keys.retain(|(_, at)| now.duration_since(*at) < Duration::from_millis(200));
        keys.iter().any(|(sent, _)| *sent == key)
    }

    fn classify(&self, event: &Event) -> Signal {
        // Our own events must never trigger the stop key or the hotkey, or a
        // sequence that types the stop key would stop itself.
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if event.is_synthetic {
            return Signal::Nothing;
        }

        let key = match event.event_type {
            EventType::KeyPress(key) => key,
            _ => return Signal::Nothing,
        };

        // Linux cannot mark a synthetic event, so keys we just sent are known
        // by time instead.
        #[cfg(target_os = "linux")]
        if self.was_self_sent(key) {
            return Signal::Nothing;
        }

        let held = self.modifiers.load(Ordering::SeqCst);

        if let Some(stop) = &self.stop_key {
            if stop.key == key && held == modifier_bits(&stop.modifiers) {
                return Signal::Stop;
            }
        }
        if let Some(trigger) = &self.trigger_key {
            if trigger.key == key && held == modifier_bits(&trigger.modifiers) {
                return Signal::Trigger;
            }
        }
        Signal::Nothing
    }

    fn track_modifiers(&self, event: &Event) {
        let (key, pressed) = match event.event_type {
            EventType::KeyPress(key) => (key, true),
            EventType::KeyRelease(key) => (key, false),
            _ => return,
        };
        let bit = modifier_bit(key);
        if bit == 0 {
            return;
        }
        if pressed {
            self.modifiers.fetch_or(bit, Ordering::SeqCst);
        } else {
            self.modifiers.fetch_and(!bit, Ordering::SeqCst);
        }
    }

    /// Records a key we are about to send, for the Linux self-check.
    pub fn note_self_sent(&self, key: Key) {
        if let Ok(mut keys) = self.self_keys.lock() {
            keys.push((key, Instant::now()));
            // Bounded: a long run should not grow this without limit.
            if keys.len() > 64 {
                keys.remove(0);
            }
        }
    }
}

/// Starts the global listener on its own thread. True when it started.
///
/// The listener is passive: it never swallows a key, so a hotkey still reaches
/// the focused app, and the stop key still does whatever it normally does.
pub fn spawn_listener(listener: Arc<Listener>) -> std::thread::JoinHandle<bool> {
    std::thread::spawn(move || {
        let handler = Arc::clone(&listener);
        let control = Arc::clone(&listener.control);
        let trigger_state = Arc::clone(&listener.trigger_state);
        rdev::listen(move |event| {
            handler.track_modifiers(&event);
            match handler.classify(&event) {
                Signal::Stop => {
                    control.stop("stop key");
                }
                Signal::Trigger => {
                    trigger_state.signal();
                }
                Signal::Nothing => {}
            }
        })
        .is_ok()
    })
}

/// How input will be delivered on this OS, for the banner.
pub fn platform_note(dry_run: bool) -> &'static str {
    if dry_run {
        return "dry run: no input will be sent";
    }
    #[cfg(target_os = "macos")]
    {
        "macOS: needs Accessibility and Input Monitoring for your terminal"
    }
    #[cfg(target_os = "windows")]
    {
        "Windows: sends input with SendInput"
    }
    #[cfg(target_os = "linux")]
    {
        "Linux: X11 only (XTest to send, XRecord to listen); Wayland is not supported"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "unsupported platform"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::parse_stroke;

    #[test]
    fn modifier_bits_add_up() {
        let stroke = parse_stroke("ctrl+shift+s").unwrap();
        assert_eq!(modifier_bits(&stroke.modifiers), BIT_CTRL | BIT_SHIFT);
    }

    #[test]
    fn no_modifiers_is_zero() {
        let stroke = parse_stroke("s").unwrap();
        assert_eq!(modifier_bits(&stroke.modifiers), 0);
    }

    #[test]
    fn modifier_keys_map_to_their_bit() {
        assert_eq!(modifier_bit(Key::MetaRight), BIT_META);
        assert_eq!(modifier_bit(Key::ShiftRight), BIT_SHIFT);
        assert_eq!(modifier_bit(Key::AltGr), BIT_ALT);
        assert_eq!(modifier_bit(Key::ControlRight), BIT_CTRL);
        assert_eq!(modifier_bit(Key::KeyS), 0);
    }

    #[test]
    fn self_sent_keys_are_bounded() {
        let listener = Listener::new(
            Arc::new(Control::new()),
            Arc::new(TriggerState::new()),
            None,
            None,
        );
        for _ in 0..200 {
            listener.note_self_sent(Key::KeyA);
        }
        assert!(listener.was_self_sent(Key::KeyA));
    }
}
