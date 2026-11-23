//! Shared state and the run loop for the clickloop window.
//!
//! The repeating happens on a worker thread that drives `clickloop`'s own
//! engine, so the window stays responsive and a hotkey can stop it. The worker
//! publishes progress as cheap atomics, which the window reads each frame.

use clickloop::control::{Control, wait_seconds};
use clickloop::keys;
use clickloop::platform;
use clickloop::sequence::{self, Action};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What the user has typed.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    pub actions: String,
    pub delay: f64,
    pub start_delay: f64,
    pub duration: f64,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            actions: "1,1,click".to_string(),
            delay: 0.0,
            start_delay: 3.0,
            duration: 5.0,
        }
    }
}

impl Form {
    /// The duration the run will use, or None for no limit.
    pub fn effective_duration(&self) -> Option<f64> {
        if self.duration > 0.0 {
            Some(self.duration)
        } else {
            None
        }
    }

    pub fn preview(&self) -> Result<Vec<Action>, String> {
        sequence::parse(&self.actions).map_err(|problem| problem.message)
    }
}

/// A short, human line describing the parsed sequence.
pub fn describe_actions(actions: &[Action]) -> String {
    sequence::describe(actions)
}

/// How long one cycle takes, from the sequence itself plus the gap after it.
///
/// Wait actions are exact; a key press or click costs the platform settle time
/// plus the event itself, which is small but not zero. This is used only for a
/// prediction, so an estimate is honest as long as it is labelled one.
pub fn predicted_cycle(actions: &[Action], delay: f64) -> Duration {
    let per_event = Duration::from_millis(6);
    let mut total = Duration::from_secs_f64(delay.max(0.0));
    for action in actions {
        total += match action {
            Action::Wait(seconds) => *seconds,
            _ => per_event,
        };
    }
    total
}

/// What the whole run will do, given the form.
#[derive(Clone, Debug, PartialEq)]
pub struct Prediction {
    /// How long one cycle takes.
    pub cycle: Duration,
    /// How long the whole run takes, or None when it has no limit.
    pub total: Option<Duration>,
    /// How many cycles it will do, or None when unbounded.
    pub cycles: Option<u64>,
}

impl Prediction {
    pub fn of(form: &Form, actions: &[Action]) -> Self {
        let cycle = predicted_cycle(actions, form.delay);
        let total = form.effective_duration().map(Duration::from_secs_f64);
        let cycles = match total {
            Some(total) if cycle.as_secs_f64() > 0.0 => {
                Some((total.as_secs_f64() / cycle.as_secs_f64()).floor() as u64)
            }
            _ => None,
        };
        Self {
            cycle,
            total,
            cycles,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Counting,
    Running,
    Stopped,
}

const PHASE_IDLE: u64 = 0;
const PHASE_COUNTING: u64 = 1;
const PHASE_RUNNING: u64 = 2;
const PHASE_STOPPED: u64 = 3;

/// What the worker thread publishes.
#[derive(Debug)]
pub struct Progress {
    pub phase: AtomicU64,
    pub cycles: AtomicU64,
    pub elapsed_millis: AtomicU64,
    /// Set when a run refused to start, or ended early.
    pub reason: Mutex<Option<String>>,
    /// Cycles expected for a finished run, for the report.
    pub target_cycles: AtomicU64,
    pub target_total_millis: AtomicU64,
    pub started: AtomicBool,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            phase: AtomicU64::new(PHASE_IDLE),
            cycles: AtomicU64::new(0),
            elapsed_millis: AtomicU64::new(0),
            reason: Mutex::new(None),
            target_cycles: AtomicU64::new(0),
            target_total_millis: AtomicU64::new(0),
            started: AtomicBool::new(false),
        }
    }
}

impl Progress {
    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::SeqCst) {
            PHASE_COUNTING => Phase::Counting,
            PHASE_RUNNING => Phase::Running,
            PHASE_STOPPED => Phase::Stopped,
            _ => Phase::Idle,
        }
    }

    pub fn set_phase(&self, phase: Phase) {
        let value = match phase {
            Phase::Idle => PHASE_IDLE,
            Phase::Counting => PHASE_COUNTING,
            Phase::Running => PHASE_RUNNING,
            Phase::Stopped => PHASE_STOPPED,
        };
        self.phase.store(value, Ordering::SeqCst);
    }

    pub fn cycles(&self) -> u64 {
        self.cycles.load(Ordering::SeqCst)
    }

    pub fn elapsed(&self) -> Duration {
        Duration::from_millis(self.elapsed_millis.load(Ordering::SeqCst))
    }

    pub fn reason(&self) -> Option<String> {
        self.reason.lock().ok().and_then(|slot| slot.clone())
    }

    pub fn set_reason(&self, reason: Option<String>) {
        if let Ok(mut slot) = self.reason.lock() {
            *slot = reason;
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self.phase(), Phase::Running | Phase::Counting)
    }

    /// How far through the run is, 0 to 1, or None when unbounded.
    ///
    /// Bounded runs use cycles done against cycles expected, which moves
    /// steadily even when a cycle contains a wait.
    pub fn fraction(&self) -> Option<f32> {
        let target = self.target_cycles.load(Ordering::SeqCst);
        if target == 0 {
            return None;
        }
        let done = self.cycles();
        Some((done as f32 / target as f32).clamp(0.0, 1.0))
    }

    /// Seconds left, from the elapsed time and the fraction done.
    pub fn remaining(&self) -> Option<Duration> {
        let total = self.target_total_millis.load(Ordering::SeqCst);
        if total == 0 {
            return None;
        }
        let total = Duration::from_millis(total);
        let elapsed = self.elapsed();
        Some(total.saturating_sub(elapsed))
    }
}

/// A handle to one run, shared between the window and its worker thread.
#[derive(Clone)]
pub struct RunHandle {
    pub control: Arc<Control>,
    pub progress: Arc<Progress>,
}

impl RunHandle {
    pub fn stop(&self) {
        self.control.stop("stopped");
    }
}

/// Starts a run on a worker thread.
///
/// Input is validated here, so the window can show a problem without starting
/// anything.
pub fn start(form: &Form) -> Result<RunHandle, String> {
    let actions = form.preview()?;
    if actions.is_empty() {
        return Err("The sequence is empty.".to_string());
    }
    if let platform::Permission::Missing(message) = platform::check_permissions() {
        return Err(message.to_string());
    }

    let control = Arc::new(Control::new());
    control.set_duration(form.effective_duration().map(Duration::from_secs_f64));

    let progress = Arc::new(Progress::default());
    let prediction = Prediction::of(form, &actions);
    progress
        .target_cycles
        .store(prediction.cycles.unwrap_or(0), Ordering::SeqCst);
    progress.target_total_millis.store(
        prediction
            .total
            .map(|total| total.as_millis() as u64)
            .unwrap_or(0),
        Ordering::SeqCst,
    );
    progress.set_phase(Phase::Counting);

    let start_delay = form.start_delay;
    let delay = form.delay;

    let handle = RunHandle {
        control: Arc::clone(&control),
        progress: Arc::clone(&progress),
    };

    std::thread::spawn(move || {
        wait_seconds(&control, Duration::from_secs_f64(start_delay));
        control.mark_started();
        progress.started.store(true, Ordering::SeqCst);
        progress.set_phase(Phase::Running);

        loop {
            if control.is_stopped() {
                break;
            }
            if control.time_limit_reached() {
                control.stop("time limit reached");
                break;
            }

            for action in &actions {
                match action {
                    Action::Wait(seconds) => wait_seconds(&control, *seconds),
                    other => {
                        if !platform::send_action(other) {
                            control.stop("the system refused an input event");
                            break;
                        }
                    }
                }
                if control.is_stopped() {
                    break;
                }
            }

            control.bump_cycles();
            progress.cycles.store(control.cycles(), Ordering::SeqCst);
            progress
                .elapsed_millis
                .store(control.elapsed().as_millis() as u64, Ordering::SeqCst);

            if control.is_stopped() {
                break;
            }
            if delay > 0.0 {
                wait_seconds(&control, Duration::from_secs_f64(delay));
            }
        }

        progress.set_reason(Some(control.reason()));
        progress
            .elapsed_millis
            .store(control.elapsed().as_millis() as u64, Ordering::SeqCst);
        progress.set_phase(Phase::Stopped);
    });

    Ok(handle)
}

/// Keys a sequence will press, so the window can warn the hotkey listener
/// about them and a run cannot trigger itself.
pub fn sequence_keys(actions: &[Action]) -> Vec<clickloop::keys::Key> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Key(stroke) => Some(stroke.press_order()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Parses a duration spec such as `5`, `0.5`, `5s` or `500ms`.
pub fn parse_seconds(text: &str) -> Option<f64> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return None;
    }
    let (number, scale) = if let Some(rest) = text.strip_suffix("ms") {
        (rest, 0.001)
    } else if let Some(rest) = text.strip_suffix('s') {
        (rest, 1.0)
    } else {
        (text.as_str(), 1.0)
    };
    let value: f64 = number.trim().parse().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(value * scale)
}

/// A key name is valid if the sequence language can parse it.
pub fn valid_key(name: &str) -> bool {
    keys::parse_stroke(name).is_some()
}

/// Formats a duration for the window: `12s`, `1:05`, `0.5s`.
pub fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds >= 60.0 {
        let whole = seconds as u64;
        format!("{}:{:02}", whole / 60, whole % 60)
    } else if (seconds.fract()).abs() < 0.05 {
        format!("{}s", seconds.round() as u64)
    } else {
        format!("{seconds:.1}s")
    }
}

/// Seconds as a short label, for settings.
pub fn format_secs(value: f64) -> String {
    if (value.fract()).abs() < 1e-9 {
        format!("{}s", value as i64)
    } else {
        format!("{value:.2}s")
    }
}

/// Tracks when a run began, so the window can show a live elapsed time between
/// worker updates.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pub began: Option<Instant>,
    pub elapsed: Duration,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            began: None,
            elapsed: Duration::ZERO,
        }
    }
}

impl Clock {
    pub fn reset(&mut self) {
        self.began = None;
        self.elapsed = Duration::ZERO;
    }

    pub fn now(&self) -> Duration {
        match self.began {
            Some(began) => began.elapsed(),
            None => self.elapsed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(actions: &str, duration: f64, delay: f64) -> Form {
        Form {
            actions: actions.to_string(),
            duration,
            delay,
            start_delay: 0.0,
        }
    }

    #[test]
    fn predicts_one_cycle_from_the_sequence() {
        let actions = sequence::parse("wait:0.5,click").unwrap();
        let cycle = predicted_cycle(&actions, 0.0);
        // A wait is exact, a click carries the settle time.
        assert!(cycle >= Duration::from_millis(500));
        assert!(cycle < Duration::from_millis(560), "{cycle:?}");
    }

    #[test]
    fn the_delay_between_cycles_counts() {
        let actions = sequence::parse("click").unwrap();
        let without = predicted_cycle(&actions, 0.0);
        let with = predicted_cycle(&actions, 1.0);
        assert_eq!(with - without, Duration::from_secs(1));
    }

    #[test]
    fn prediction_multiplies_cycles_for_a_bounded_run() {
        let actions = sequence::parse("wait:1").unwrap();
        let prediction = Prediction::of(&form("wait:1", 10.0, 0.0), &actions);
        assert_eq!(prediction.total, Some(Duration::from_secs(10)));
        // Ten seconds of one-second cycles is ten cycles, and the prediction
        // must not sell the user short.
        assert_eq!(prediction.cycles, Some(10));
    }

    #[test]
    fn an_unbounded_run_predicts_no_total() {
        let actions = sequence::parse("click").unwrap();
        let prediction = Prediction::of(&form("click", 0.0, 0.0), &actions);
        assert_eq!(prediction.total, None);
        assert_eq!(prediction.cycles, None);
    }

    #[test]
    fn progress_fraction_and_remaining() {
        let progress = Progress::default();
        progress.target_cycles.store(10, Ordering::SeqCst);
        progress.target_total_millis.store(5000, Ordering::SeqCst);
        assert_eq!(progress.fraction(), Some(0.0));

        progress.cycles.store(5, Ordering::SeqCst);
        progress.elapsed_millis.store(2500, Ordering::SeqCst);
        assert_eq!(progress.fraction(), Some(0.5));
        assert_eq!(progress.remaining(), Some(Duration::from_millis(2500)));
    }

    #[test]
    fn unbounded_progress_has_no_fraction() {
        let progress = Progress::default();
        assert_eq!(progress.fraction(), None);
        assert_eq!(progress.remaining(), None);
    }

    #[test]
    fn fraction_never_exceeds_one() {
        let progress = Progress::default();
        progress.target_cycles.store(2, Ordering::SeqCst);
        progress.cycles.store(9, Ordering::SeqCst);
        assert_eq!(progress.fraction(), Some(1.0));
    }

    #[test]
    fn seconds_parse_with_units() {
        assert_eq!(parse_seconds("5"), Some(5.0));
        assert_eq!(parse_seconds("0.5"), Some(0.5));
        assert_eq!(parse_seconds("5s"), Some(5.0));
        assert_eq!(parse_seconds("500ms"), Some(0.5));
        assert_eq!(parse_seconds("0"), Some(0.0));
        assert_eq!(parse_seconds(""), None);
        assert_eq!(parse_seconds("abc"), None);
        assert_eq!(parse_seconds("-1"), None);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(format_duration(Duration::from_secs(12)), "12s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(format_duration(Duration::from_millis(500)), "0.5s");
    }

    #[test]
    fn key_names_validate() {
        assert!(valid_key("f5"));
        assert!(valid_key("cmd+s"));
        assert!(!valid_key("notakey"));
    }

    #[test]
    fn sequence_keys_lists_every_press() {
        let actions = sequence::parse("cmd+s,click,f1").unwrap();
        let keys = sequence_keys(&actions);
        assert_eq!(keys.len(), 3);
    }
}
