//! Stop control and hotkey signalling.
//!
//! The Swift script used `NSLock` around two booleans. The same shape is kept
//! here, with `std::sync` primitives.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Why the run stopped. Set once, read by the main loop and the final report.
#[derive(Debug)]
pub struct Control {
    stopped: AtomicBool,
    reason: Mutex<String>,
    started: Mutex<Option<Instant>>,
    duration: Mutex<Option<Duration>>,
    cycles: AtomicU64,
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}

impl Control {
    pub fn new() -> Self {
        Self {
            stopped: AtomicBool::new(false),
            reason: Mutex::new(String::new()),
            started: Mutex::new(None),
            duration: Mutex::new(None),
            cycles: AtomicU64::new(0),
        }
    }

    /// Stops the run. The first reason wins, matching the Swift `StopFlag`.
    pub fn stop(&self, reason: &str) -> bool {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return false;
        }
        if let Ok(mut slot) = self.reason.lock() {
            *slot = reason.to_string();
        }
        true
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub fn reason(&self) -> String {
        self.reason
            .lock()
            .map(|slot| slot.clone())
            .unwrap_or_default()
    }

    /// Marks the moment the first cycle starts, which the time limit counts from.
    pub fn mark_started(&self) {
        if let Ok(mut slot) = self.started.lock() {
            *slot = Some(Instant::now());
        }
    }

    /// A `None` duration means no time limit.
    pub fn set_duration(&self, duration: Option<Duration>) {
        if let Ok(mut slot) = self.duration.lock() {
            *slot = duration;
        }
    }

    /// True once the elapsed time since the first cycle reaches the limit.
    pub fn time_limit_reached(&self) -> bool {
        let limit = match self.duration.lock() {
            Ok(slot) => *slot,
            Err(_) => None,
        };
        let limit = match limit {
            Some(limit) => limit,
            None => return false,
        };
        let started = match self.started.lock() {
            Ok(slot) => *slot,
            Err(_) => None,
        };
        match started {
            Some(started) => started.elapsed() >= limit,
            None => false,
        }
    }

    pub fn bump_cycles(&self) -> u64 {
        self.cycles.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn cycles(&self) -> u64 {
        self.cycles.load(Ordering::SeqCst)
    }

    pub fn elapsed(&self) -> Duration {
        match self.started.lock() {
            Ok(slot) => slot.map(|started| started.elapsed()).unwrap_or_default(),
            Err(_) => Duration::default(),
        }
    }
}

/// One pending hotkey press, taken by the run loop.
///
/// The Swift version allowed only one pending press and dropped presses that
/// arrived while a sequence was running. The same rule is kept: a press during
/// a run is not queued.
pub struct TriggerState {
    pending: AtomicBool,
    busy: AtomicBool,
}

impl Default for TriggerState {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerState {
    pub fn new() -> Self {
        Self {
            pending: AtomicBool::new(false),
            busy: AtomicBool::new(false),
        }
    }

    /// Called from the listener thread on a hotkey press.
    pub fn signal(&self) {
        if !self.busy.load(Ordering::SeqCst) {
            self.pending.store(true, Ordering::SeqCst);
        }
    }

    /// Takes a pending press, marking the state busy. False when nothing waited.
    pub fn take(&self) -> bool {
        if !self.pending.swap(false, Ordering::SeqCst) {
            return false;
        }
        self.busy.store(true, Ordering::SeqCst);
        true
    }

    pub fn finish(&self) {
        self.busy.store(false, Ordering::SeqCst);
    }
}

/// Waits up to `seconds`, returning early when the run is stopped or the time
/// limit is reached. This is the responsive wait the Swift script used, so the
/// stop key lands during a `wait:` action.
pub fn wait_seconds(control: &Arc<Control>, seconds: Duration) {
    let deadline = Instant::now() + seconds;
    loop {
        if control.is_stopped() || control.time_limit_reached() {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        let remaining = deadline - now;
        // Short steps keep the stop key responsive without spinning.
        let step = remaining.min(Duration::from_millis(5));
        std::thread::sleep(step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_reason_wins() {
        let control = Control::new();
        assert!(control.stop("time limit reached"));
        assert!(!control.stop("Enter in terminal"));
        assert_eq!(control.reason(), "time limit reached");
    }

    #[test]
    fn no_limit_never_reached() {
        let control = Control::new();
        control.mark_started();
        assert!(!control.time_limit_reached());
    }

    #[test]
    fn limit_reached_after_marking() {
        let control = Control::new();
        control.set_duration(Some(Duration::from_millis(20)));
        // Not started yet, so the limit does not apply.
        assert!(!control.time_limit_reached());
        control.mark_started();
        std::thread::sleep(Duration::from_millis(40));
        assert!(control.time_limit_reached());
    }

    #[test]
    fn trigger_drops_presses_while_busy() {
        let trigger = TriggerState::new();

        // A press while idle waits for the run loop.
        trigger.signal();
        assert!(trigger.take());

        // A press that arrives during a running sequence is dropped, because
        // the Swift TriggerState ignores a signal while busy.
        trigger.signal();
        trigger.finish();
        assert!(!trigger.take());

        // Once the run has finished, the next press is picked up again.
        trigger.signal();
        assert!(trigger.take());
        trigger.finish();

        // Two presses in one idle window collapse into the one pending press.
        trigger.signal();
        trigger.signal();
        assert!(trigger.take());
        trigger.finish();
        assert!(!trigger.take());
    }

    #[test]
    fn wait_returns_early_when_stopped() {
        let control = Arc::new(Control::new());
        let stopper = Arc::clone(&control);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            stopper.stop("stop key");
        });
        let started = Instant::now();
        wait_seconds(&control, Duration::from_secs(5));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(control.is_stopped());
    }
}
