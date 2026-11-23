//! The window: one column, one status, and a panel for history and reference.
//!
//! Two screens live here: the run screen, and a side panel that shows either
//! the past sequences or a reference for the sequence language. Both are the
//! same view, so the progress bar and the button never disagree.

use crate::hotkey::{Bindings, Router, Signal, Slot};
use crate::state::{
    self, Form, Phase, RunHandle, describe_actions as desc, format_duration, format_secs,
    parse_seconds, start, valid_key,
};
use crate::store;
use crate::theme::*;
use gpui::prelude::*;
use gpui::{
    Context, ElementId, Entity, IntoElement, MouseButton, Render, SharedString, Task, Window, div,
    px,
};
use gpui_elements::editable_text::{EditableTextState, StringStorage, text_input};
use std::time::Duration;

/// Ready-made sequences, each one a working phrase.
const PRESETS: &[(&str, &str)] = &[
    ("Click", "click"),
    ("Default", "1,1,click"),
    ("Keys 1-2-3", "1,2,3"),
    ("Repeat 5", "(1,1,click)*5"),
];

/// Which panel is open over the run screen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    None,
    History,
    Reference,
}

/// The reference, as rows of (token, meaning). One source for the whole table.
const REFERENCE: &[(&str, &str)] = &[
    ("click", "left click where the pointer is"),
    ("right-click", "right click"),
    ("a  f5  space", "any key name, from --list-keys"),
    ("cmd+s", "a key with modifiers held"),
    ("wait:0.5", "pause for half a second"),
    ("1*3", "repeat one action three times"),
    ("(a,b)*4", "repeat a group as a unit"),
    ("((a,b)*2,c)*3", "groups nest and multiply"),
    ("cmd/meta/super/win", "the same modifier on every OS"),
];

pub struct ClickloopWindow {
    form: Form,
    run: Option<RunHandle>,
    router: Router,
    bindings: Bindings,
    /// Set when the last attempt to start failed.
    error: Option<String>,
    panel: Panel,
    history: Vec<String>,
    /// Where the elapsed clock reads from between worker updates.
    since: Option<std::time::Instant>,
    _ticker: Option<Task<()>>,
    input: Option<Entity<EditableTextState>>,
}

impl ClickloopWindow {
    pub fn new(router: Router, _cx: &mut Context<Self>) -> Self {
        Self {
            form: Form::default(),
            run: None,
            bindings: store::load_bindings(),
            router,
            error: None,
            panel: Panel::None,
            history: store::load_history(),
            since: None,
            _ticker: None,
            input: None,
        }
    }

    /// A window pinned to a state, for screenshot rendering.
    pub fn previewed(
        form: Form,
        phase: Phase,
        cycles: u64,
        elapsed: Duration,
        target: Option<(u64, Duration)>,
        reason: Option<String>,
        error: Option<String>,
        bindings: Bindings,
        history: Vec<String>,
        panel: Panel,
    ) -> Self {
        let progress = state::Progress::default();
        progress.set_phase(phase);
        progress
            .cycles
            .store(cycles, std::sync::atomic::Ordering::SeqCst);
        progress.elapsed_millis.store(
            elapsed.as_millis() as u64,
            std::sync::atomic::Ordering::SeqCst,
        );
        if let Some((cycles, total)) = target {
            progress
                .target_cycles
                .store(cycles, std::sync::atomic::Ordering::SeqCst);
            progress.target_total_millis.store(
                total.as_millis() as u64,
                std::sync::atomic::Ordering::SeqCst,
            );
        }
        progress.set_reason(reason);
        let control = std::sync::Arc::new(clickloop::control::Control::new());
        let since = if matches!(phase, Phase::Running | Phase::Counting) {
            Some(std::time::Instant::now() - elapsed)
        } else {
            None
        };
        Self {
            form,
            run: Some(RunHandle {
                control,
                progress: std::sync::Arc::new(progress),
            }),
            router: Router::dormant(&bindings),
            bindings,
            error,
            panel,
            history,
            since,
            _ticker: None,
            input: None,
        }
    }

    fn phase(&self) -> Phase {
        self.run
            .as_ref()
            .map(|run| run.progress.phase())
            .unwrap_or(Phase::Idle)
    }

    fn live(&self) -> bool {
        matches!(self.phase(), Phase::Running | Phase::Counting)
    }

    fn cycles(&self) -> u64 {
        self.run
            .as_ref()
            .map(|run| run.progress.cycles())
            .unwrap_or(0)
    }

    /// Elapsed time, taken from the clock while a run is live.
    fn elapsed(&self) -> Duration {
        match &self.since {
            Some(since) if self.live() => since.elapsed(),
            _ => self
                .run
                .as_ref()
                .map(|run| run.progress.elapsed())
                .unwrap_or_default(),
        }
    }

    fn prediction(&self) -> Option<state::Prediction> {
        self.form
            .preview()
            .ok()
            .map(|actions| state::Prediction::of(&self.form, &actions))
    }

    fn start_run(&mut self, cx: &mut Context<Self>) {
        if self.live() {
            return;
        }
        self.error = None;
        self.sync_form(cx);
        match start(&self.form) {
            Ok(handle) => {
                eprintln!("clickloop: run started: {}", self.form.actions);
                self.history = store::remember(&self.form.actions);
                self.since = Some(std::time::Instant::now());
                self.run = Some(handle);
                self.tick(cx);
            }
            Err(message) => {
                self.error = Some(message);
                cx.notify();
            }
        }
    }

    fn stop_run(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = &self.run {
            run.stop();
            eprintln!(
                "clickloop: run stopped after {} cycles",
                run.progress.cycles()
            );
        }
        cx.notify();
    }

    fn sync_form(&mut self, cx: &mut Context<Self>) {
        if let Some(input) = &self.input {
            self.form.actions = input.read(cx).as_str().to_string();
        }
    }

    fn set_sequence(&mut self, sequence: &str, cx: &mut Context<Self>) {
        if self.live() {
            return;
        }
        self.form.actions = sequence.to_string();
        self.error = None;
        if let Some(input) = &self.input {
            input.update(cx, |state, cx| state.emplace(sequence, cx));
        }
        cx.notify();
    }

    /// Handles the hotkey channel, which the OS feeds whether or not this
    /// window has focus.
    fn drain_hotkeys(&mut self, cx: &mut Context<Self>) {
        while let Some(signal) = self.router.poll() {
            match signal {
                Signal::Start => self.start_run(cx),
                Signal::Stop => self.stop_run(cx),
                Signal::Captured(slot, label) => {
                    match slot {
                        Slot::Start => self.bindings.start = label,
                        Slot::Stop => self.bindings.stop = label,
                    }
                    self.router.set_bindings(&self.bindings);
                    store::save_bindings(&self.bindings);
                    cx.notify();
                }
            }
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self._ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                match this.update(cx, |this, cx| {
                    // Hotkeys arrive on their own thread; this is where they land.
                    this.drain_hotkeys(cx);
                    if this.live() {
                        cx.notify();
                        true
                    } else {
                        this.since = None;
                        cx.notify();
                        false
                    }
                }) {
                    Ok(true) => {}
                    _ => break,
                }
            }
        }));
    }
}

impl Render for ClickloopWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Pick up hotkeys even when idle, so F5 works from a cold start.
        self.drain_hotkeys(cx);

        // The editable box is created once and reused across frames.
        let input_id = ElementId::from("sequence-input");
        let initial = self.form.actions.clone();
        let state = EditableTextState::use_keyed_init(input_id, window, cx, move |_, _| {
            StringStorage::from(initial)
        });
        if self.input.is_none() {
            self.input = Some(state.clone());
        }
        self.form.actions = state.read(cx).as_str().to_string();

        let theme = Theme::dark();
        let html = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg)
            .text_color(theme.text)
            .font_family(".SystemUIFont")
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .px(px(22.0))
                    .pt(px(18.0))
                    .pb(px(18.0))
                    .gap(px(13.0))
                    .child(header(&theme))
                    .child(self.hotkey_row(&theme, cx))
                    .child(sequence_card(&theme, &state, &self.form, &self.error))
                    .child(self.progress_band(&theme))
                    .child(self.recent_row(&theme, cx))
                    .child(div().flex_1())
                    .child(self.footer(&theme, cx)),
            );

        if self.panel == Panel::None {
            html.into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .size_full()
                .bg(theme.bg)
                .text_color(theme.text)
                .font_family(".SystemUIFont")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .px(px(22.0))
                        .pt(px(18.0))
                        .pb(px(18.0))
                        .gap(px(13.0))
                        .child(self.panel_header(&theme, cx))
                        .child(self.panel_body(&theme, cx))
                        .child(div().flex_1())
                        .child(self.panel_footer(&theme, cx)),
                )
                .into_any_element()
        }
    }
}

fn header(theme: &Theme) -> impl IntoElement {
    div()
        .flex()
        .items_baseline()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(16.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("clickloop"),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(theme.text_faint)
                .child("keys and clicks, repeated"),
        )
}

/// A key name, drawn as a keycap so it reads as a key and not as prose.
fn keycap(theme: &Theme, name: &str) -> impl IntoElement {
    div()
        .px(px(7.0))
        .py(px(2.0))
        .rounded(px(RADIUS_KEYCAP))
        .bg(theme.surface)
        .border_1()
        .border_color(theme.line_strong)
        .text_size(px(12.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text_dim)
        .child(SharedString::from(name.to_string()))
}

fn section_label(theme: &Theme, text: &'static str) -> impl IntoElement {
    div()
        .text_size(px(9.5))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text_faint)
        .child(text)
}

impl ClickloopWindow {
    /// The two hotkeys, each one clickable to rebind. This is also the only
    /// place the start/stop keys are stated.
    fn hotkey_row(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let capturing = self.router.is_capturing();
        let live = self.router.is_live();
        let hint = match capturing {
            Some(Slot::Start) => "Press any key for start",
            Some(Slot::Stop) => "Press any key for stop",
            None if !live => "Hotkeys need input permission",
            None => "",
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(hotkey_chip(
                        theme,
                        "start",
                        &self.bindings.start,
                        capturing == Some(Slot::Start),
                        cx,
                        Slot::Start,
                    ))
                    .child(hotkey_chip(
                        theme,
                        "stop",
                        &self.bindings.stop,
                        capturing == Some(Slot::Stop),
                        cx,
                        Slot::Stop,
                    )),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_faint)
                    .child(SharedString::from(hint.to_string())),
            )
    }

    fn progress_band(&self, theme: &Theme) -> impl IntoElement {
        let prediction = self.prediction();
        let phase = self.phase();
        let live = self.live();

        // Where the bar sits, and what it says. One statement per fact: the
        // bar carries progress, the caption carries time.
        let fraction: f32 = if live {
            self.run
                .as_ref()
                .and_then(|run| run.progress.fraction())
                .unwrap_or(0.0)
        } else if phase == Phase::Stopped {
            1.0
        } else {
            0.0
        };

        let caption = if let Some(error) = &self.error {
            format!("{error}")
        } else if live {
            let done = self.cycles();
            let total = self
                .run
                .as_ref()
                .and_then(|run| run.progress.remaining())
                .map(format_duration);
            match (prediction.as_ref().and_then(|p| p.cycles), total) {
                (Some(expected), Some(left)) => {
                    format!("{done} of {expected} · {left} left")
                }
                _ => format!("{done} cycles · {}", format_duration(self.elapsed())),
            }
        } else if phase == Phase::Stopped {
            let reason = self
                .run
                .as_ref()
                .and_then(|run| run.progress.reason())
                .unwrap_or_else(|| "stopped".to_string());
            format!(
                "{reason} · {} cycles · {}",
                self.cycles(),
                format_duration(self.elapsed())
            )
        } else {
            match &prediction {
                Some(p) if p.total.is_some() => format!(
                    "{} cycles in {}",
                    p.cycles.unwrap_or(0),
                    format_duration(p.total.unwrap_or_default())
                ),
                Some(p) => format!("about {} per cycle", format_duration(p.cycle)),
                None => "Check the sequence".to_string(),
            }
        };

        let ok = self.error.is_none() && prediction.is_some();

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(progress_bar(theme, fraction, live, ok))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(if self.error.is_some() {
                        theme.danger
                    } else {
                        theme.text_faint
                    })
                    .child(SharedString::from(caption)),
            )
    }

    /// The last few sequences, one tap to restore. Empty until something has
    /// been run, so the first open stays quiet.
    fn recent_row(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let live = self.live();
        let mut row = div().flex().flex_col().gap(px(5.0));
        if self.history.is_empty() || live {
            return row;
        }
        row = row.child(section_label(theme, "RECENT"));
        let mut chips = div().flex().flex_wrap().gap(px(6.0));
        for (index, item) in self.history.iter().take(3).enumerate() {
            let sequence = item.clone();
            let selected = *item == self.form.actions;
            chips = chips.child(
                div()
                    .id(SharedString::from(format!("recent-{index}")))
                    .px(px(9.0))
                    .py(px(4.0))
                    .rounded(px(RADIUS_CHIP))
                    .bg(if selected {
                        theme.accent_soft
                    } else {
                        theme.surface
                    })
                    .border_1()
                    .border_color(if selected { theme.accent } else { theme.line })
                    .text_size(px(11.5))
                    .font_family("Menlo")
                    .text_color(if selected {
                        theme.accent
                    } else {
                        theme.text_dim
                    })
                    .cursor_pointer()
                    .hover_style(move |style| style.bg(theme.surface_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| this.set_sequence(&sequence, cx)),
                    )
                    .child(SharedString::from(truncate(item, 22))),
            );
        }
        row.child(chips)
    }

    fn footer(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let live = self.live();
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(self.settings_chip(theme, "DELAY", format_secs(self.form.delay)))
                            .child(self.settings_chip(
                                theme,
                                "FOR",
                                if self.form.duration > 0.0 {
                                    format_secs(self.form.duration)
                                } else {
                                    "no limit".to_string()
                                },
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.0))
                            .child(panel_link(theme, "History", cx, Panel::History, self.panel))
                            .child(panel_link(
                                theme,
                                "Syntax",
                                cx,
                                Panel::Reference,
                                self.panel,
                            )),
                    ),
            )
            .child(
                div()
                    .id("run-button")
                    .w_full()
                    .h(px(42.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(RADIUS_CARD))
                    .bg(if live { theme.surface } else { theme.accent })
                    .border_1()
                    .border_color(if live {
                        theme.line_strong
                    } else {
                        theme.accent
                    })
                    .text_color(if live { theme.text } else { theme.on_accent })
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover_style(move |style| {
                        style.bg(if live {
                            theme.surface_hover
                        } else {
                            theme.accent_hover
                        })
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if this.live() {
                                this.stop_run(cx);
                            } else {
                                this.start_run(cx);
                            }
                        }),
                    )
                    .child(if live { "Stop" } else { "Run" }),
            )
    }

    fn settings_chip(&self, theme: &Theme, label: &'static str, value: String) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .child(section_label(theme, label))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_dim)
                    .child(SharedString::from(value)),
            )
    }

    fn panel_header(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.panel {
            Panel::History => "Past sequences",
            _ => "Sequence syntax",
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .id("panel-close")
                    .px(px(9.0))
                    .py(px(3.0))
                    .rounded(px(RADIUS_CHIP))
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line)
                    .text_size(px(11.5))
                    .text_color(theme.text_dim)
                    .cursor_pointer()
                    .hover_style(move |style| style.bg(theme.surface_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.panel = Panel::None;
                            cx.notify();
                        }),
                    )
                    .child("Back"),
            )
    }

    fn panel_body(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        match self.panel {
            Panel::History => {
                let mut list = div().flex().flex_col().gap(px(4.0));
                if self.history.is_empty() {
                    list = list.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_faint)
                            .child("Nothing yet. Run a sequence and it lands here."),
                    );
                }
                for (index, item) in self.history.iter().take(9).enumerate() {
                    let sequence = item.clone();
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("history-{index}")))
                            .px(px(10.0))
                            .py(px(7.0))
                            .rounded(px(RADIUS_CHIP))
                            .bg(theme.surface)
                            .border_1()
                            .border_color(if *item == self.form.actions {
                                theme.accent
                            } else {
                                theme.line
                            })
                            .text_size(px(12.5))
                            .font_family("Menlo")
                            .text_color(theme.text_dim)
                            .cursor_pointer()
                            .hover_style(move |style| style.bg(theme.surface_hover))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.set_sequence(&sequence, cx);
                                    this.panel = Panel::None;
                                }),
                            )
                            .child(SharedString::from(item.clone())),
                    );
                }
                list.into_any_element()
            }
            _ => {
                let mut rows = div().flex().flex_col().gap(px(5.0));
                for (token, meaning) in REFERENCE {
                    rows = rows.child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .w(px(132.0))
                                    .flex_shrink_0()
                                    .text_size(px(11.5))
                                    .font_family("Menlo")
                                    .text_color(theme.accent)
                                    .child(*token),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(11.5))
                                    .text_color(theme.text_dim)
                                    .child(*meaning),
                            ),
                    );
                }
                rows.into_any_element()
            }
        }
    }

    fn panel_footer(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = theme;
        let _ = cx;
        div().flex().items_center().gap(px(6.0)).child(
            div()
                .text_size(px(10.5))
                .text_color(theme.text_faint)
                .child("Keys: click, right-click, wait:0.5, a name from --list-keys"),
        )
    }
}

/// The editable sequence, and one line saying what it does or what is wrong.
fn sequence_card(
    theme: &Theme,
    state: &Entity<EditableTextState>,
    form: &Form,
    error: &Option<String>,
) -> impl IntoElement {
    let description = match (error, form.preview()) {
        (Some(error), _) => error.clone(),
        (None, Ok(actions)) => desc(&actions),
        (None, Err(problem)) => problem,
    };
    let valid = error.is_none() && form.preview().is_ok();
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .w_full()
                .px(px(11.0))
                .py(px(9.0))
                .rounded(px(RADIUS_CARD))
                .bg(theme.surface)
                .border_1()
                .border_color(theme.line)
                .child(
                    text_input(ElementId::from("sequence-input"))
                        .state(state.downgrade())
                        .caret_blink_interval_500ms()
                        .placeholder_color(theme.text_faint)
                        .text_color(theme.text)
                        .text_size(px(14.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .w_full(),
                ),
        )
        .child(
            div()
                .px(px(11.0))
                .text_size(px(11.0))
                .text_color(if valid {
                    theme.text_faint
                } else {
                    theme.danger
                })
                .child(SharedString::from(description)),
        )
}

/// Shortens a sequence for a chip, keeping it recognisable.
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn hotkey_chip(
    theme: &Theme,
    label: &'static str,
    value: &str,
    capturing: bool,
    cx: &mut Context<ClickloopWindow>,
    slot: Slot,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("hotkey-{label}")))
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .child(
            div()
                .text_size(px(9.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text_faint)
                .child(label.to_uppercase()),
        )
        .child(
            div()
                .px(px(7.0))
                .py(px(2.0))
                .rounded(px(RADIUS_KEYCAP))
                .bg(if capturing {
                    theme.accent_soft
                } else {
                    theme.surface
                })
                .border_1()
                .border_color(if capturing {
                    theme.accent
                } else {
                    theme.line_strong
                })
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(if capturing {
                    theme.accent
                } else {
                    theme.text_dim
                })
                .child(SharedString::from(if capturing {
                    "press".to_string()
                } else {
                    value.to_string()
                })),
        )
        .hover_style(move |style| style.opacity(0.85))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.router.capture(slot);
                cx.notify();
            }),
        )
}

fn panel_link(
    theme: &Theme,
    label: &'static str,
    cx: &mut Context<ClickloopWindow>,
    target: Panel,
    current: Panel,
) -> impl IntoElement {
    let active = current == target;
    div()
        .id(SharedString::from(format!("link-{label}")))
        .text_size(px(11.5))
        .text_color(if active {
            theme.accent
        } else {
            theme.text_faint
        })
        .cursor_pointer()
        .hover_style(move |style| style.text_color(theme.accent))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.panel = if active { Panel::None } else { target };
                cx.notify();
            }),
        )
        .child(label)
}

/// A thin progress bar: the accent fills from the left.
fn progress_bar(theme: &Theme, fraction: f32, live: bool, ok: bool) -> impl IntoElement {
    let fraction = fraction.clamp(0.0, 1.0);
    let fill = if !ok { 0.0 } else { fraction };
    let color = if !ok {
        theme.danger
    } else if live {
        theme.accent
    } else if fraction >= 1.0 {
        theme.accent
    } else {
        theme.accent
    };
    div()
        .w_full()
        .h(px(3.0))
        .rounded_full()
        .bg(theme.track)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(gpui::relative(fill))
                .rounded_full()
                .bg(color),
        )
}

/// Parses a duration the user typed; kept here so the check and the value
/// agree.
#[allow(dead_code)]
fn seconds_or_zero(text: &str) -> f64 {
    parse_seconds(text).unwrap_or(0.0)
}

#[allow(dead_code)]
fn sequence_is_valid(form: &Form) -> bool {
    form.preview().is_ok() && valid_key(&"f5")
}
