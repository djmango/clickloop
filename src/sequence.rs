//! The sequence expression language.
//!
//! Grammar, unchanged from the Swift original:
//!
//! ```text
//! list  := item ("," item)*
//! item  := group ("*" number)?
//! group := token | "(" list ")"
//! ```
//!
//! A parenthesised group multiplies as a unit: `(1,1,click)*3` is those three
//! actions, three times. Groups nest and each level multiplies the one inside
//! it: `((a,b)*2)*3` is a,b twelve times. A plain item also takes a `*N`
//! suffix.

use crate::keys::{parse_stroke, Stroke};
use std::fmt;
use std::time::Duration;

/// The largest expansion a sequence may produce, so a runaway nest cannot
/// exhaust memory.
pub const MAX_SEQUENCE_ACTIONS: usize = 1_000_000;

/// A single thing to do in a cycle.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Press and release a key, holding its modifiers.
    Key(Stroke),
    /// Click a mouse button at the current pointer position.
    Click(ClickButton, &'static str),
    /// Wait, staying responsive to the stop key and the time limit.
    Wait(Duration),
}

/// The mouse buttons the sequence language can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickButton {
    Left,
    Right,
}

/// A parse failure, with the message shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceError {
    pub message: String,
}

impl fmt::Display for SequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SequenceError {}

impl SequenceError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Parses one action token, or returns the reason it is not one.
fn parse_action(token: &str) -> Option<Action> {
    match token {
        "click" | "left-click" | "leftclick" | "lclick" => {
            return Some(Action::Click(ClickButton::Left, "left-click"));
        }
        "right-click" | "rightclick" | "rclick" => {
            return Some(Action::Click(ClickButton::Right, "right-click"));
        }
        "middle-click" | "middleclick" | "mclick" => {
            return Some(Action::Click(ClickButton::Left, "left-click"));
        }
        _ => {}
    }

    if let Some(rest) = token
        .strip_prefix("wait:")
        .or_else(|| token.strip_prefix("sleep:"))
    {
        let seconds: f64 = rest.trim().parse().ok()?;
        if !seconds.is_finite() || seconds < 0.0 {
            return None;
        }
        return Some(Action::Wait(Duration::from_secs_f64(seconds)));
    }

    parse_stroke(token).map(Action::Key)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    error: Option<String>,
}

impl Parser {
    fn new(spec: &str) -> Self {
        Self {
            chars: spec.chars().collect(),
            pos: 0,
            error: None,
        }
    }

    fn skip_spaces(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn fail(&mut self, message: impl Into<String>) -> Option<Vec<Action>> {
        self.error = Some(message.into());
        None
    }

    /// Reads one action token up to the next delimiter: `,` `(` `)` `*`.
    fn read_token(&mut self) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == ',' || c == '(' || c == ')' || c == '*' {
                break;
            }
            out.push(c);
            self.pos += 1;
        }
        out.trim().to_lowercase()
    }

    /// Reads a comma separated list, stopping at `)` or the end of input.
    fn parse_list(&mut self) -> Option<Vec<Action>> {
        let mut actions: Vec<Action> = Vec::new();
        loop {
            self.skip_spaces();
            if self.peek() == Some(')') {
                break;
            }
            let item = self.parse_item()?;
            actions.extend(item);
            if actions.len() > MAX_SEQUENCE_ACTIONS {
                return self.fail(format!(
                    "the sequence expands to more than {MAX_SEQUENCE_ACTIONS} actions"
                ));
            }
            self.skip_spaces();
            if self.peek() == Some(',') {
                self.pos += 1;
                continue;
            }
            break;
        }
        Some(actions)
    }

    fn parse_item(&mut self) -> Option<Vec<Action>> {
        self.skip_spaces();
        let mut base: Vec<Action> = if self.peek() == Some('(') {
            self.pos += 1;
            let inner = self.parse_list()?;
            self.skip_spaces();
            if self.peek() != Some(')') {
                return self.fail("missing ')'");
            }
            self.pos += 1;
            if inner.is_empty() {
                return self.fail("an empty group '()' does nothing");
            }
            inner
        } else {
            let raw = self.read_token();
            if raw.is_empty() {
                return self.fail("empty item before ',' or '*'");
            }
            match parse_action(&raw) {
                Some(action) => vec![action],
                None => {
                    return self.fail(format!(
                        "unknown action '{raw}'. Run with --list-keys to see the key names."
                    ));
                }
            }
        };

        // Optional "*N" repeat on this item or on the whole group.
        self.skip_spaces();
        if self.peek() == Some('*') {
            self.pos += 1;
            self.skip_spaces();
            let mut num = String::new();
            while let Some(c) = self.peek() {
                if !c.is_ascii_digit() {
                    break;
                }
                num.push(c);
                self.pos += 1;
            }
            let repeat = match num.parse::<usize>() {
                Ok(n) if n > 0 && n <= 100_000 => n,
                _ => return self.fail("bad repeat count after '*'; use 1 to 100000"),
            };
            let total = base.len().checked_mul(repeat);
            match total {
                Some(total) if total <= MAX_SEQUENCE_ACTIONS => {}
                _ => {
                    return self.fail(format!(
                        "the sequence expands to more than {MAX_SEQUENCE_ACTIONS} actions"
                    ));
                }
            }
            let mut out: Vec<Action> = Vec::with_capacity(base.len() * repeat);
            for _ in 0..repeat {
                out.extend_from_slice(&base);
            }
            base = out;
        }

        Some(base)
    }
}

/// Parses a whole sequence specification.
pub fn parse(spec: &str) -> Result<Vec<Action>, SequenceError> {
    let mut parser = Parser::new(spec);
    let actions = parser.parse_list();
    if let Some(problem) = parser.error.clone() {
        return Err(SequenceError::new(problem));
    }
    let actions = match actions {
        Some(actions) => actions,
        None => return Err(SequenceError::new("cannot parse the sequence")),
    };
    parser.skip_spaces();
    if let Some(leftover) = parser.peek() {
        return Err(SequenceError::new(format!(
            "unexpected '{leftover}' in the sequence"
        )));
    }
    if actions.is_empty() {
        return Err(SequenceError::new("the sequence is empty"));
    }
    Ok(actions)
}

fn action_label(action: &Action) -> String {
    match action {
        Action::Key(stroke) => format!("press {}", stroke.label),
        Action::Click(_, label) => (*label).to_string(),
        Action::Wait(seconds) => format!("wait {}s", format_seconds(seconds.as_secs_f64())),
    }
}

/// Seconds as a short decimal, without a trailing `.0`.
fn format_seconds(seconds: f64) -> String {
    if (seconds.fract()).abs() < f64::EPSILON {
        format!("{}", seconds as i64)
    } else {
        let text = format!("{seconds:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// A short human readable form of the actions, with runs collapsed to `xN`.
pub fn describe(actions: &[Action]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut run_label = String::new();
    let mut run_count = 0usize;

    fn flush(parts: &mut Vec<String>, label: &str, count: usize) {
        if count == 0 {
            return;
        }
        if count > 1 {
            parts.push(format!("{label} x{count}"));
        } else {
            parts.push(label.to_string());
        }
    }

    for action in actions {
        let text = action_label(action);
        if text == run_label {
            run_count += 1;
        } else {
            flush(&mut parts, &run_label, run_count);
            run_label = text;
            run_count = 1;
        }
    }
    flush(&mut parts, &run_label, run_count);

    if parts.len() > 40 {
        let head = parts[..40].join(", ");
        return format!("{head}, ... ({} actions total)", actions.len());
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifier;

    fn count(spec: &str) -> usize {
        parse(spec).unwrap().len()
    }

    #[test]
    fn default_sequence() {
        let actions = parse("1,1,click").unwrap();
        assert_eq!(actions.len(), 3);
        assert!(matches!(actions[0], Action::Key(_)));
        assert!(matches!(actions[2], Action::Click(ClickButton::Left, _)));
    }

    #[test]
    fn whitespace_is_ignored() {
        assert_eq!(count(" 1 , 1 , click "), 3);
        assert_eq!(count("1,  1,click"), 3);
    }

    #[test]
    fn repeats() {
        assert_eq!(count("1*2"), 2);
        assert_eq!(count("space*3"), 3);
        assert_eq!(count("space*3,click"), 4);
    }

    #[test]
    fn groups_multiply() {
        assert_eq!(count("(1,1,click)*3"), 9);
        assert_eq!(count("((a,b)*2)*3"), 12);
        assert_eq!(count("((a,b)*2,space)*4"), 20);
    }

    #[test]
    fn nested_without_repeat() {
        assert_eq!(count("(a,(b,c))"), 3);
    }

    #[test]
    fn waits() {
        let actions = parse("wait:0.5").unwrap();
        match &actions[0] {
            Action::Wait(duration) => {
                assert!((duration.as_secs_f64() - 0.5).abs() < 1e-9);
            }
            other => panic!("expected a wait, got {other:?}"),
        }
        assert_eq!(count("sleep:2"), 1);
        assert_eq!(count("wait:0"), 1);
    }

    #[test]
    fn chords_in_sequence() {
        let actions = parse("cmd+s,shift+space").unwrap();
        match (&actions[0], &actions[1]) {
            (Action::Key(first), Action::Key(second)) => {
                assert_eq!(first.modifiers, vec![Modifier::Meta]);
                assert_eq!(second.modifiers, vec![Modifier::Shift]);
            }
            other => panic!("expected two key strokes, got {other:?}"),
        }
    }

    #[test]
    fn errors_match_the_original() {
        let missing = parse("(a,b").unwrap_err();
        assert_eq!(missing.message, "missing ')'");

        let empty_group = parse("()").unwrap_err();
        assert_eq!(empty_group.message, "an empty group '()' does nothing");

        let bad_repeat = parse("a*0").unwrap_err();
        assert_eq!(
            bad_repeat.message,
            "bad repeat count after '*'; use 1 to 100000"
        );

        let unknown = parse("notakey").unwrap_err();
        assert!(unknown.message.starts_with("unknown action 'notakey'"));

        let stray = parse("a)b").unwrap_err();
        assert_eq!(stray.message, "unexpected ')' in the sequence");

        // The Swift original reports an empty spec as an empty item, not as an
        // empty sequence; an empty sequence comes from a group that resolves
        // to nothing, which its parser rejects earlier.
        let empty = parse("").unwrap_err();
        assert_eq!(empty.message, "empty item before ',' or '*'");

        let empty_item = parse("a,,b").unwrap_err();
        assert_eq!(empty_item.message, "empty item before ',' or '*'");
    }

    #[test]
    fn expansion_cap_holds() {
        // 100000 repeats of a 3 action group is 300000, under the cap.
        assert_eq!(count("(a,b,c)*100000"), 300_000);
        // A repeat above 100000 is refused before it can allocate.
        assert!(parse("(a,b,c)*100001").is_err());
        // Nesting multiplies past the cap and is refused.
        assert!(parse("((a,b,c)*100000)*100000").is_err());
    }

    #[test]
    fn describe_collapses_runs() {
        let actions = parse("1,1,click").unwrap();
        assert_eq!(describe(&actions), "press 1 x2, left-click");
    }

    #[test]
    fn describe_truncates_long_sequences() {
        let actions = parse(&("a,click,".repeat(60) + "a")).unwrap();
        let text = describe(&actions);
        assert!(text.contains("actions total"), "{text}");
    }

    #[test]
    fn describe_shows_waits_compactly() {
        let actions = parse("wait:0.5,wait:2").unwrap();
        assert_eq!(describe(&actions), "wait 0.5s, wait 2s");
    }
}
