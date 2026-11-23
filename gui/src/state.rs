//! Shared state and the run loop for the clickloop window.
//!
//! The window owns a [`Form`] (what the user typed) and a [`Progress`] (what the
//! tool is doing). The repeating happens on a worker thread that drives
//! `clickloop`'s own engine, so the window stays responsive and can stop it.

use clickloop::control::{Control, TriggerState, wait_seconds};
use clickloop::keys::{self, Stroke};
use clickloop::platform::{self, Listener};
use clickloop::sequence::{self, Action};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the user has typed.
#[derive(Clone, Debug)]
pub struct Form {
    pub actions: String,
    pub delay: f64,
    pub start_delay: f64,
    pub duration: f64,
    pub count: Option<u64>,
    pub stop_key: String,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            actions: "1,1,click".to_string(),
            delay: 0.0,
            start_delay: 3.0,
            duration: 5.0,
            count: None,
            stop_key: "escape".to_string(),
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

    pub fn stop_stroke(&self) -> Option<Stroke> {
        keys::parse_stroke(&self.stop_key)
    }

    /// Parses the sequence, for the live preview under the field.
    pub fn preview(&self) -> Result<Vec<Action>, String> {
        sequence::parse(&self.actions).map_err(|problem| problem.message)
    }
}

/// A short, human line describing the parsed sequence.
pub fn describe_actions(actions: &[Action]) -> String {
    sequence::describe(actions)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Counting,
    Running,
    Stopped,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Idle => "READY",
            Phase::Counting => "GET READY",
            Phase::Running => "RUNNING",
            Phase::Stopped => "STOPPED",
        }
    }
}

const PHASE_IDLE: u8 = 0;
const PHASE_COUNTING: u8 = 1;
const PHASE_RUNNING: u8 = 2;
const PHASE_STOPPED: u8 = 3;

/// What the worker thread reports back, read by the window each frame.
#[derive(Debug)]
pub struct Progress {
    pub phase: AtomicU64,
    pub cycles: AtomicU64,
    pub elapsed_millis: AtomicU64,
    pub reason: Mutex<Option<String>>,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            phase: AtomicU64::new(PHASE_IDLE as u64),
            cycles: AtomicU64::new(0),
            elapsed_millis: AtomicU64::new(0),
            reason: Mutex::new(None),
        }
    }
}

impl Progress {
    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::SeqCst) {
            x if x == PHASE_COUNTING as u64 => Phase::Counting,
            x if x == PHASE_RUNNING as u64 => Phase::Running,
            x if x == PHASE_STOPPED as u64 => Phase::Stopped,
            _ => Phase::Idle,
        }
    }

    pub fn set_phase(&self, phase: Phase) {
        let value = match phase {
            Phase::Idle => PHASE_IDLE,
            Phase::Counting => PHASE_COUNTING,
            Phase::Running => PHASE_RUNNING,
            Phase::Stopped => PHASE_STOPPED,
        } as u64;
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

    /// Used by screenshot rendering to pin a finished state.
    pub fn set_reason_for_preview(&self, reason: Option<String>) {
        if let Ok(mut slot) = self.reason.lock() {
            *slot = reason;
        }
    }

    fn set_reason(&self, reason: String) {
        if let Ok(mut slot) = self.reason.lock() {
            *slot = Some(reason);
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.phase(), Phase::Running | Phase::Counting)
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
        self.control.stop("stopped in the window");
    }
}

/// Starts a run on a worker thread.
///
/// The sequence and the stop key are validated here, so the window can show the
/// error without a round trip.
pub fn start(form: &Form) -> Result<RunHandle, String> {
    let actions = form.preview()?;
    if actions.is_empty() {
        return Err("The sequence is empty.".to_string());
    }

    let stop_key = match form.stop_key.trim() {
        "" => None,
        spec if matches!(spec.to_lowercase().as_str(), "none" | "off") => None,
        spec => match keys::parse_stroke(spec) {
            Some(stroke) => Some(stroke),
            None => {
                return Err(format!(
                    "\"{spec}\" is not a key name. Use \"none\" to turn the stop key off."
                ));
            }
        },
    };

    if let platform::Permission::Missing(message) = platform::check_permissions() {
        return Err(message.to_string());
    }

    let control = Arc::new(Control::new());
    control.set_duration(form.effective_duration().map(Duration::from_secs_f64));

    let progress = Arc::new(Progress::default());
    progress.set_phase(Phase::Counting);

    let count = form.count;
    let delay = form.delay;
    let start_delay = form.start_delay;

    // The listener carries the stop key, exactly as the command line tool does.
    if let Some(stroke) = stop_key.clone() {
        let listener = Arc::new(Listener::new(
            Arc::clone(&control),
            Arc::new(TriggerState::new()),
            Some(stroke),
            None,
        ));
        let _ = platform::spawn_listener(listener);
    }

    let handle = RunHandle {
        control: Arc::clone(&control),
        progress: Arc::clone(&progress),
    };

    std::thread::spawn(move || {
        wait_seconds(&control, Duration::from_secs_f64(start_delay));
        control.mark_started();
        progress.set_phase(Phase::Running);

        loop {
            if control.is_stopped() {
                break;
            }
            if control.time_limit_reached() {
                control.stop("time limit reached");
                break;
            }
            if let Some(limit) = count {
                if control.cycles() >= limit {
                    control.stop("cycle count reached");
                    break;
                }
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

        progress.set_reason(control.reason());
        progress
            .elapsed_millis
            .store(control.elapsed().as_millis() as u64, Ordering::SeqCst);
        progress.set_phase(Phase::Stopped);
    });

    Ok(handle)
}

/// Bytes written by a hot run, used to keep the window repainting while it works.
pub fn is_active(handle: &Option<RunHandle>) -> bool {
    match handle {
        Some(handle) => handle.progress.is_running(),
        None => false,
    }
}

/// True while a run should keep the UI polling for progress.
pub fn watch_flag(handle: &Option<RunHandle>) -> bool {
    is_active(handle)
}

/// A tiny helper for tests and for the window: the phase a run is in.
#[allow(dead_code)]
pub fn phase_of(handle: &Option<RunHandle>) -> Phase {
    handle
        .as_ref()
        .map(|handle| handle.progress.phase())
        .unwrap_or(Phase::Idle)
}

#[allow(dead_code)]
pub fn active_bool() -> AtomicBool {
    AtomicBool::new(false)
}
