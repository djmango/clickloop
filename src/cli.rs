//! The command line surface, and the run loop.

use crate::control::{wait_seconds, Control, TriggerState};
use crate::keys::parse_stroke;
use crate::platform::{self, Listener};
use crate::sequence::{describe, parse, Action};
use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

/// Options, with the same defaults as the Swift script.
pub struct Options {
    pub delay: f64,
    pub start_delay: f64,
    pub duration: f64,
    pub duration_specified: bool,
    pub count: Option<u64>,
    pub dry_run: bool,
    pub sequence: String,
    pub stop_key: String,
    pub hotkey: Option<String>,
    pub confirm_keys: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            delay: 0.0,
            start_delay: 3.0,
            duration: 5.0,
            duration_specified: false,
            count: None,
            dry_run: false,
            sequence: "1,1,click".to_string(),
            stop_key: "escape".to_string(),
            hotkey: None,
            confirm_keys: "8,9,0".to_string(),
        }
    }
}

fn usage() {
    println!(
        r#"Usage: clickloop [options]

Repeats a sequence of key presses and mouse clicks.
Works on macOS, Windows and Linux (X11).
Default sequence: press 1, press 1, left-click at the pointer position.

Safety options:
  --duration SECONDS   Stop after this time. Default: 5, or unlimited with --hotkey.
                       Use 0 for no time limit.
  --stop-key KEY       Key that stops the script. Default: escape. Use "none" to disable.
  --count N            Stop after N cycles. Default: unlimited.
  --start-delay SEC    Seconds before the first cycle. Default: 3.
  --confirm-keys LIST  Keys that need a yes before the run starts, for keys that are
                       risky to press by accident. Default: "8,9,0". Use "none" to
                       disable. Only a real key press asks; a repeat count never does.

Sequence options:
  --actions LIST       Comma separated sequence. Default: "1,1,click".
                       Items: a key name, "click", "right-click", "wait:0.5".
                       Repeat an item with "*N": "1*2" means press 1 twice.
                       Group with parentheses and multiply the whole group:
                       "(1,1,click)*3" runs those three actions three times.
                       Groups nest and each level multiplies: "((a,b)*2,space)*4".
                       Modifiers join with "+": "cmd+s", "shift+space".
                       cmd/meta/super/win all mean the same modifier key, so one
                       sequence runs unchanged on every OS.
  --delay SECONDS      Seconds between cycles. Default: 0.
  --hotkey KEY         Run the sequence once per key press instead of repeating.
                       The hotkey also passes through to the focused app.
  --list-keys          Print every key name and stop.

Other:
  --dry-run            Print the actions, send no input.
  -h, --help           Print this text.

Examples:
  clickloop                                # 5 seconds, then stop
  clickloop --duration 30 --delay 0.2
  clickloop --actions "space*3,click" --stop-key f12
  clickloop --actions "(1,1,click)*5"      # a group, five times
  clickloop --duration 0                   # no time limit, escape stops it

Stops on: the time limit, the cycle count, the stop key, Control-C, Enter in this terminal.
"#
    );
}

fn list_keys() {
    let names = crate::keys::key_names();
    println!("Key names ({}):", names.len());
    let mut line = String::new();
    for name in names {
        line.push_str(&format!("{name:<18}"));
        if line.len() >= 90 {
            println!("{line}");
            line.clear();
        }
    }
    if !line.is_empty() {
        println!("{line}");
    }
    println!(
        "\nModifiers: cmd (meta, super, win), shift, alt (opt), ctrl, fn. \
         Join a modifier to a key with \"+\", for example cmd+s."
    );
}

fn fail(message: &str) -> i32 {
    eprintln!("Error: {message}");
    1
}

fn note(message: &str) {
    eprintln!("{message}");
}

/// Parses the arguments, returning the options or the message to show.
pub fn parse_args(args: &[String]) -> Result<Outcome, String> {
    let mut options = Options::default();
    let mut index = 0;

    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--help" | "-h" => return Ok(Outcome::Help),
            "--dry-run" => {
                options.dry_run = true;
                index += 1;
            }
            "--list-keys" => return Ok(Outcome::ListKeys),
            "--duration" | "--delay" | "--start-delay" | "--count" | "--actions" | "--stop-key"
            | "--hotkey" | "--confirm-keys" => {
                let value = match args.get(index + 1) {
                    Some(value) => value.clone(),
                    None => return Err(format!("Missing value for {arg}")),
                };
                match arg {
                    "--count" => match value.parse::<u64>() {
                        Ok(n) if n > 0 => options.count = Some(n),
                        _ => return Err("--count must be a positive integer".to_string()),
                    },
                    "--actions" => options.sequence = value,
                    "--stop-key" => options.stop_key = value,
                    "--hotkey" => options.hotkey = Some(value),
                    "--confirm-keys" => options.confirm_keys = value,
                    _ => {
                        let seconds: f64 = match value.parse() {
                            Ok(n) if value.parse::<f64>().map(f64::is_finite).unwrap_or(false) => n,
                            _ => {
                                return Err(format!(
                                    "{arg} must be a nonnegative number of seconds"
                                ));
                            }
                        };
                        if !seconds.is_finite() || seconds < 0.0 {
                            return Err(format!("{arg} must be a nonnegative number of seconds"));
                        }
                        match arg {
                            "--delay" => options.delay = seconds,
                            "--start-delay" => options.start_delay = seconds,
                            _ => {
                                options.duration = seconds;
                                options.duration_specified = true;
                            }
                        }
                    }
                }
                index += 2;
            }
            other => {
                return Err(format!(
                    "Unknown option: {other}. Run with --help for the options."
                ));
            }
        }
    }

    Ok(Outcome::Run(options))
}

/// What the command line asked for.
pub enum Outcome {
    Help,
    ListKeys,
    Run(Options),
}

fn is_off(spec: &str) -> bool {
    matches!(
        spec.trim().to_lowercase().as_str(),
        "none" | "off" | "disable"
    )
}

/// The stop keys a run will honour, or the reason the spec is wrong.
fn resolve_stop_key(spec: &str) -> Result<Option<crate::keys::Stroke>, String> {
    if is_off(spec) {
        return Ok(None);
    }
    match parse_stroke(spec) {
        Some(stroke) => Ok(Some(stroke)),
        None => Err(format!(
            "Unknown --stop-key value '{spec}'. Run with --list-keys to see the key names."
        )),
    }
}

/// The confirm keys a run will ask about. Duplicates collapse, first spelling wins.
fn resolve_confirm_keys(spec: &str) -> Result<Vec<crate::keys::Stroke>, String> {
    if spec.trim().is_empty() || is_off(spec) {
        return Ok(Vec::new());
    }
    let mut strokes: Vec<crate::keys::Stroke> = Vec::new();
    for raw in spec.to_lowercase().split(',') {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        let stroke = match parse_stroke(name) {
            Some(stroke) => stroke,
            None => {
                return Err(format!(
                    "Unknown --confirm-keys value '{name}'. Run with --list-keys to see the key names."
                ));
            }
        };
        if !strokes.iter().any(|known| known.key == stroke.key) {
            strokes.push(stroke);
        }
    }
    Ok(strokes)
}

/// Keys this sequence presses that are on the confirm list, in first-use order.
fn risky_keys(actions: &[Action], confirm: &[crate::keys::Stroke]) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for action in actions {
        if let Action::Key(stroke) = action {
            if confirm.iter().any(|risky| risky.key == stroke.key)
                && !labels.contains(&stroke.label)
            {
                labels.push(stroke.label.clone());
            }
        }
    }
    labels
}

fn ask_yes(labels: &[String]) -> bool {
    eprint!(
        "This sequence presses {}. Do you want to continue? [y/N] ",
        labels.join(", ")
    );
    let _ = io::stderr().flush();
    let mut answer = String::new();
    match io::stdin().read_line(&mut answer) {
        Ok(0) | Err(_) => false,
        Ok(_) => matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"),
    }
}

/// Runs the tool and returns the exit code.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(&args) {
        Ok(Outcome::Help) => {
            usage();
            return 0;
        }
        Ok(Outcome::ListKeys) => {
            list_keys();
            return 0;
        }
        Ok(Outcome::Run(options)) => options,
        Err(message) => return fail(&message),
    };
    run_with(options)
}

fn run_with(options: Options) -> i32 {
    // Resolve the stop key.
    let stop_key = match resolve_stop_key(&options.stop_key) {
        Ok(key) => key,
        Err(message) => return fail(&message),
    };

    // Resolve the hotkey, and make an unbounded run when one is given.
    let mut duration = options.duration;
    let trigger_key = match &options.hotkey {
        Some(spec) => {
            if !options.duration_specified {
                duration = 0.0;
            }
            let stroke = match parse_stroke(spec) {
                Some(stroke) => stroke,
                None => {
                    return fail(&format!(
                        "Unknown --hotkey value '{spec}'. Run with --list-keys."
                    ));
                }
            };
            if let Some(stop) = &stop_key {
                if stop.key == stroke.key && stop.modifiers == stroke.modifiers {
                    return fail("--hotkey conflicts with --stop-key. Choose different keys.");
                }
            }
            Some(stroke)
        }
        None => None,
    };

    // Resolve the confirm keys.
    let confirm_keys = match resolve_confirm_keys(&options.confirm_keys) {
        Ok(keys) => keys,
        Err(message) => return fail(&message),
    };

    // Parse the sequence.
    let actions = match parse(&options.sequence) {
        Ok(actions) => actions,
        Err(problem) => return fail(&problem.message),
    };

    // Check the permissions this OS needs.
    if !options.dry_run {
        if let platform::Permission::Missing(message) = platform::check_permissions() {
            return fail(message);
        }
    }

    // A risky key needs a yes before the run.
    if !options.dry_run && !confirm_keys.is_empty() {
        let labels = risky_keys(&actions, &confirm_keys);
        if !labels.is_empty() && !ask_yes(&labels) {
            note("Stopped: confirmation declined.");
            return 0;
        }
    }

    let control = Arc::new(Control::new());
    let trigger_state = Arc::new(TriggerState::new());
    if duration > 0.0 {
        control.set_duration(Some(Duration::from_secs_f64(duration)));
    }

    // Control-C, and SIGTERM where the platform has it.
    install_signal_handlers(Arc::clone(&control));

    // Enter in the terminal stops the run. A background thread, so a run that
    // is not on a terminal still works.
    if io::stdin().is_terminal() {
        let stdin_control = Arc::clone(&control);
        std::thread::spawn(move || {
            let stdin = io::stdin();
            let mut line = String::new();
            loop {
                line.clear();
                match stdin.lock().read_line(&mut line) {
                    Ok(0) => return,
                    Ok(_) => {
                        if stdin_control.stop("Enter in terminal") {
                            return;
                        }
                        if stdin_control.is_stopped() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
    }

    // The global listener, which carries the stop key and the hotkey.
    let needs_listener = !options.dry_run && (stop_key.is_some() || trigger_key.is_some());
    let listener = Arc::new(Listener::new(
        Arc::clone(&control),
        Arc::clone(&trigger_state),
        stop_key.clone(),
        trigger_key.clone(),
    ));
    let mut listener_handle = None;
    if needs_listener {
        listener_handle = Some(platform::spawn_listener(Arc::clone(&listener)));
        // Give the tap a moment to install before the banner claims it works.
        std::thread::sleep(Duration::from_millis(150));
        if trigger_key.is_some() && !listener_alive(&listener_handle) {
            note(
                "Warning: the hotkey listener is unavailable. Give your terminal Accessibility \
                 and Input Monitoring permissions, then try again.",
            );
        } else if !listener_alive(&listener_handle) {
            note(
                "Warning: the stop key is not available. Give your terminal Accessibility and \
                 Input Monitoring. Control-C and Enter still stop the script.",
            );
        }
    }

    // Last-resort watchdog: fires only if the main loop is stuck.
    if duration > 0.0 {
        let watchdog_control = Arc::clone(&control);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(duration) + Duration::from_secs(5));
            if !watchdog_control.is_stopped() {
                note("Watchdog: the main loop did not stop. Hard exit.");
                std::process::exit(0);
            }
        });
    }

    // Banner.
    let stop_label = match &stop_key {
        Some(stroke) => stroke.label.clone(),
        None => "disabled".to_string(),
    };
    let mut banner = format!("Sequence: {}.", describe(&actions));
    if duration > 0.0 {
        banner.push_str(&format!(" Runs for {} seconds.", trim_number(duration)));
    } else {
        banner.push_str(" No time limit.");
    }
    if let Some(n) = options.count {
        banner.push_str(&format!(" Stops after {n} cycles."));
    }
    if let Some(hotkey) = &trigger_key {
        banner.push_str(&format!(
            " Hotkey: {}; one sequence per press (key passes through).",
            hotkey.label
        ));
    }
    banner.push_str(&format!(" Stop key: {stop_label}."));
    note(&banner);
    note(&format!(
        "Starting in {} seconds. Focus the target window.",
        trim_number(options.start_delay)
    ));
    note(platform::platform_note(options.dry_run));
    if needs_listener && listener_alive(&listener_handle) && stop_key.is_some() {
        note(&format!(
            "Press {stop_label} to stop at any time. Control-C and Enter also stop."
        ));
    }

    // Start delay, then the cycles.
    wait_seconds(&control, Duration::from_secs_f64(options.start_delay));
    control.mark_started();

    let count = options.count;
    loop {
        if control.is_stopped() {
            break;
        }
        if control.time_limit_reached() {
            control.stop("time limit reached");
            break;
        }
        if let Some(n) = count {
            if control.cycles() >= n {
                control.stop("cycle count reached");
                break;
            }
        }
        if trigger_key.is_some() && !trigger_state.take() {
            wait_seconds(&control, Duration::from_millis(20));
            continue;
        }

        if options.dry_run {
            println!("[dry-run] {}", describe(&actions));
        } else {
            for action in &actions {
                match action {
                    Action::Wait(seconds) => wait_seconds(&control, *seconds),
                    other => {
                        if let Action::Key(stroke) = other {
                            // Linux has no synthetic marker, so record what we
                            // send for the listener to discount.
                            #[cfg(target_os = "linux")]
                            {
                                for key in stroke.press_order() {
                                    listener.note_self_sent(key);
                                }
                            }
                            let _ = stroke;
                        }
                        if !platform::send_action(other) {
                            note("Error: the OS refused an input event.");
                            control.stop("input refused");
                            break;
                        }
                    }
                }
                if control.is_stopped() {
                    break;
                }
            }
        }

        control.bump_cycles();
        if trigger_key.is_some() {
            trigger_state.finish();
        }
        if control.is_stopped() {
            break;
        }
        if options.delay > 0.0 {
            wait_seconds(&control, Duration::from_secs_f64(options.delay));
        }
    }

    note(&format!(
        "Stopped: {}. Cycles: {}. Elapsed: {:.2} seconds.",
        control.reason(),
        control.cycles(),
        control.elapsed().as_secs_f64()
    ));

    // The listener thread runs a blocking OS loop, so it is not joined: the
    // process is done once the run is done.
    drop(listener_handle);
    0
}

fn listener_alive(handle: &Option<std::thread::JoinHandle<bool>>) -> bool {
    match handle {
        Some(handle) => !handle.is_finished(),
        None => false,
    }
}

/// Control-C, plus SIGTERM on Unix.
fn install_signal_handlers(control: Arc<Control>) {
    let first = Arc::clone(&control);
    let second = Arc::clone(&control);
    let ok = ctrlc::set_handler(move || {
        // The first signal stops the run; a second one exits at once, so a
        // stuck loop cannot trap the user.
        if !first.stop("Control-C") {
            std::process::exit(130);
        }
    });
    let _ = &second;
    if ok.is_err() {
        // A handler is already installed; the stop key and Enter still work.
    }
}

/// A number as the Swift original prints a `Double`: a whole number still shows
/// its `.0`, so "Runs for 5.0 seconds" reads the same in both programs.
fn trim_number(value: f64) -> String {
    let text = format!("{value}");
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

/// Set when the process has already reported a signal, so the exit is clean.
pub static SIGNALLED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_match_the_swift_script() {
        let options = Options::default();
        assert_eq!(options.sequence, "1,1,click");
        assert_eq!(options.stop_key, "escape");
        assert_eq!(options.confirm_keys, "8,9,0");
        assert_eq!(options.duration, 5.0);
        assert_eq!(options.start_delay, 3.0);
        assert_eq!(options.delay, 0.0);
        assert!(!options.duration_specified);
        assert!(options.count.is_none());
    }

    #[test]
    fn parses_every_option() {
        let parsed = parse_args(&args(&[
            "--duration",
            "30",
            "--delay",
            "0.2",
            "--start-delay",
            "1",
            "--count",
            "5",
            "--actions",
            "space*3",
            "--stop-key",
            "f12",
            "--hotkey",
            "cmd+k",
            "--confirm-keys",
            "none",
        ]));
        match parsed {
            Ok(Outcome::Run(options)) => {
                assert_eq!(options.duration, 30.0);
                assert!(options.duration_specified);
                assert_eq!(options.delay, 0.2);
                assert_eq!(options.start_delay, 1.0);
                assert_eq!(options.count, Some(5));
                assert_eq!(options.sequence, "space*3");
                assert_eq!(options.stop_key, "f12");
                assert_eq!(options.hotkey.as_deref(), Some("cmd+k"));
                assert_eq!(options.confirm_keys, "none");
            }
            _ => panic!("expected a run"),
        }
    }

    #[test]
    fn help_and_list_keys_short_circuit() {
        assert!(matches!(parse_args(&args(&["-h"])), Ok(Outcome::Help)));
        assert!(matches!(
            parse_args(&args(&["--list-keys"])),
            Ok(Outcome::ListKeys)
        ));
    }

    #[test]
    fn rejects_bad_values() {
        assert!(parse_args(&args(&["--count", "0"])).is_err());
        assert!(parse_args(&args(&["--count", "x"])).is_err());
        assert!(parse_args(&args(&["--duration", "-1"])).is_err());
        assert!(parse_args(&args(&["--duration"])).is_err());
        assert!(parse_args(&args(&["--nope"])).is_err());
    }

    #[test]
    fn stop_key_none_disables() {
        assert!(resolve_stop_key("none").unwrap().is_none());
        assert!(resolve_stop_key("off").unwrap().is_none());
        assert!(resolve_stop_key("disable").unwrap().is_none());
        assert!(resolve_stop_key("escape").unwrap().is_some());
        assert!(resolve_stop_key("notakey").is_err());
    }

    #[test]
    fn confirm_keys_collapse_duplicates() {
        let keys = resolve_confirm_keys("8,9,0,8").unwrap();
        assert_eq!(keys.len(), 3);
        assert!(resolve_confirm_keys("none").unwrap().is_empty());
        assert!(resolve_confirm_keys("").unwrap().is_empty());
        assert!(resolve_confirm_keys("8,notakey").is_err());
    }

    #[test]
    fn risky_keys_are_found_without_repeats_counting() {
        let actions = parse("1*5,8,click").unwrap();
        let confirm = resolve_confirm_keys("8,9,0").unwrap();
        assert_eq!(risky_keys(&actions, &confirm), vec!["8".to_string()]);

        // A repeat count alone never asks.
        let quiet = parse("1*9,2").unwrap();
        assert!(risky_keys(&quiet, &confirm).is_empty());
    }

    #[test]
    fn numbers_match_swift_double_formatting() {
        assert_eq!(trim_number(5.0), "5.0");
        assert_eq!(trim_number(30.0), "30.0");
        assert_eq!(trim_number(0.5), "0.5");
        assert_eq!(trim_number(0.0), "0.0");
        assert_eq!(trim_number(0.25), "0.25");
    }
}
