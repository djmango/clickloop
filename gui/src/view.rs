//! The window: a small, quiet panel that runs one sequence.
//!
//! Every element is a plain `div` with a token from [`crate::theme`], so the
//! panel reads as one design. Nothing here needs an icon font or an image.

use crate::state::{Form, Phase, RunHandle, describe_actions as desc, start};
use crate::theme::*;
use gpui::prelude::*;
use gpui::{
    Context, ElementId, Entity, IntoElement, MouseButton, Render, SharedString, Task, Window, div,
    px, rgb,
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

pub struct ClickloopWindow {
    form: Form,
    run: Option<RunHandle>,
    error: Option<String>,
    _ticker: Option<Task<()>>,
    theme: Theme,
    /// The live editable sequence box.
    input: Option<Entity<EditableTextState>>,
}

impl ClickloopWindow {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            form: Form::default(),
            run: None,
            error: None,
            _ticker: None,
            theme: Theme::dark(),
            input: None,
        }
    }

    /// A window pinned to a state, for screenshot rendering.
    ///
    /// The preview shows the same view code the app shows, so a screenshot
    /// cannot drift from the real panel.
    pub fn previewed(
        form: Form,
        phase: Phase,
        cycles: u64,
        elapsed: Duration,
        reason: Option<String>,
        error: Option<String>,
    ) -> Self {
        let progress = crate::state::Progress::default();
        progress.set_phase(phase);
        progress
            .cycles
            .store(cycles, std::sync::atomic::Ordering::SeqCst);
        progress.elapsed_millis.store(
            elapsed.as_millis() as u64,
            std::sync::atomic::Ordering::SeqCst,
        );
        progress.set_reason_for_preview(reason);
        let control = std::sync::Arc::new(clickloop::control::Control::new());
        Self {
            form,
            run: Some(RunHandle {
                control,
                progress: std::sync::Arc::new(progress),
            }),
            error,
            _ticker: None,
            theme: Theme::dark(),
            input: None,
        }
    }

    /// An idle window over `form`, for screenshot rendering.
    pub fn previewed_default(form: Form) -> Self {
        Self {
            form,
            run: None,
            error: None,
            _ticker: None,
            theme: Theme::dark(),
            input: None,
        }
    }

    fn phase(&self) -> Phase {
        self.run
            .as_ref()
            .map(|run| run.progress.phase())
            .unwrap_or(Phase::Idle)
    }

    fn is_running(&self) -> bool {
        matches!(self.phase(), Phase::Running | Phase::Counting)
    }

    fn on_run(&mut self, cx: &mut Context<Self>) {
        if self.is_running() {
            return;
        }
        self.sync_form_from_input(cx);
        self.error = None;
        match start(&self.form) {
            Ok(handle) => {
                self.run = Some(handle);
                self.tick(cx);
            }
            Err(message) => {
                self.error = Some(message);
                cx.notify();
            }
        }
    }

    fn on_stop(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = &self.run {
            run.stop();
        }
        cx.notify();
    }

    /// Pulls the typed text into the form, so the preview and the run agree.
    fn sync_form_from_input(&mut self, cx: &mut Context<Self>) {
        if let Some(input) = &self.input {
            self.form.actions = input.read(cx).as_str().to_string();
        }
    }

    fn on_preset(&mut self, sequence: &str, cx: &mut Context<Self>) {
        if self.is_running() {
            return;
        }
        self.form.actions = sequence.to_string();
        self.error = None;
        if let Some(input) = &self.input {
            input.update(cx, |state, cx| {
                state.emplace(sequence, cx);
            });
        }
        cx.notify();
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if !self.is_running() {
            return;
        }
        self._ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                match this.update(cx, |this, cx| {
                    cx.notify();
                    this.is_running()
                }) {
                    Ok(true) => {}
                    _ => break,
                }
            }
        }));
    }
}

impl ClickloopWindow {
    /// The panel body, rendered against this window's own context.
    ///
    /// Split out so a screenshot wrapper can paint the exact same view.
    pub fn panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let phase = self.phase();
        let running = self.is_running();

        let preview = self.form.preview();
        let (preview_line, preview_ok) = match &preview {
            Ok(actions) => (desc(actions), true),
            Err(problem) => (problem.clone(), false),
        };

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
                    .px(px(24.0))
                    .pt(px(20.0))
                    .pb(px(20.0))
                    .gap(px(15.0))
                    .child(header(&theme, phase))
                    .child(sequence_card(
                        &theme,
                        &state,
                        &preview_line,
                        preview_ok,
                        running,
                    ))
                    .child(presets(&theme, running, &self.form.actions, cx))
                    .child(timing_row(&theme, &self.form))
                    .child(div().flex_1())
                    .child(footer(
                        &theme,
                        self.run.as_ref(),
                        running,
                        self.error.as_deref(),
                        cx,
                    )),
            )
    }
}

impl Render for ClickloopWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.panel(window, cx)
    }
}

fn header(theme: &Theme, phase: Phase) -> impl IntoElement {
    // A status should carry colour meaning: accent while live, amber while
    // counting down, red once stopped, and neutral only when nothing has run.
    let (dot, text) = match phase {
        Phase::Running => (theme.accent, theme.accent),
        Phase::Counting => (theme.warn, theme.warn),
        Phase::Stopped => (theme.danger, theme.text_muted),
        Phase::Idle => (theme.accent, theme.text_muted),
    };
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(20.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("clickloop"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text_faint)
                        .child("Repeat keys and clicks, anywhere"),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.0))
                .px(px(11.0))
                .py(px(5.0))
                .rounded_full()
                .bg(theme.panel)
                .border_1()
                .border_color(theme.border)
                .child(div().size(px(7.0)).rounded_full().bg(dot))
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(text)
                        .child(phase.label()),
                ),
        )
}

fn section_label(theme: &Theme, text: &'static str) -> impl IntoElement {
    div()
        .text_size(px(9.5))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text_faint)
        .child(text)
}

fn sequence_card(
    theme: &Theme,
    state: &Entity<EditableTextState>,
    preview_line: &str,
    preview_ok: bool,
    running: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(7.0))
        .child(section_label(theme, "SEQUENCE"))
        .child(
            div()
                .w_full()
                .px(px(12.0))
                .py(px(11.0))
                .rounded(px(RADIUS_CARD))
                .bg(theme.panel)
                .border_1()
                .border_color(theme.border)
                .child(
                    text_input(ElementId::from("sequence-input"))
                        .state(state.downgrade())
                        .caret_blink_interval_500ms()
                        .placeholder_color(theme.text_faint)
                        .text_color(theme.text)
                        .text_size(px(15.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .w_full(),
                ),
        )
        .child(
            div()
                .px(px(13.0))
                .text_size(px(11.0))
                .text_color(if preview_ok {
                    theme.text_faint
                } else {
                    theme.danger
                })
                .child(SharedString::from(if running {
                    "Running. Press the stop key or the button to stop.".to_string()
                } else {
                    preview_line.to_string()
                })),
        )
}

fn presets(
    theme: &Theme,
    running: bool,
    current: &str,
    cx: &mut Context<ClickloopWindow>,
) -> impl IntoElement {
    let mut row = div().flex().flex_col().gap(px(7.0));
    if !running {
        row = row.child(section_label(theme, "START FROM"));
    }
    let active = current.trim();
    let mut chips = div().flex().flex_wrap().gap(px(6.0));
    for (label, sequence) in PRESETS {
        // A chip marks itself when the field holds exactly its sequence, so the
        // preset row doubles as a readout of where the sequence came from.
        let selected = active == *sequence;
        let sequence = sequence.to_string();
        chips = chips.child(
            div()
                .id(SharedString::from(format!("preset-{label}")))
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(RADIUS_CHIP))
                .bg(if selected {
                    theme.accent_soft
                } else {
                    theme.panel
                })
                .border_1()
                .border_color(if selected { theme.accent } else { theme.border })
                .text_size(px(11.5))
                .font_weight(if selected {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_color(if selected {
                    theme.accent
                } else {
                    theme.text_muted
                })
                .cursor_pointer()
                .when(!running, |this| {
                    this.hover_style(move |style| {
                        style.bg(theme.panel_hover).text_color(theme.text)
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.on_preset(&sequence, cx)),
                )
                .child(*label),
        );
    }
    row.child(chips)
}

fn timing_row(theme: &Theme, form: &Form) -> impl IntoElement {
    let stop = if form.stop_key.trim().is_empty() {
        "none".to_string()
    } else {
        form.stop_key.clone()
    };
    div()
        .w_full()
        .px(px(13.0))
        .py(px(11.0))
        .rounded(px(RADIUS_CARD))
        .bg(theme.panel)
        .border_1()
        .border_color(theme.border)
        .flex()
        .items_end()
        .gap(px(26.0))
        .child(stat(theme, "DELAY", format_secs(form.delay)))
        .child(stat(
            theme,
            "FOR",
            if form.duration > 0.0 {
                format_secs(form.duration)
            } else {
                "no limit".to_string()
            },
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(section_label(theme, "STOP"))
                .child(keycap(theme, &stop)),
        )
}

/// A key name, drawn as a keycap so it reads as a key and not as prose.
fn keycap(theme: &Theme, name: &str) -> impl IntoElement {
    div()
        .px(px(7.0))
        .py(px(2.0))
        .rounded(px(RADIUS_KEYCAP))
        .bg(theme.keycap)
        .border_1()
        .border_color(theme.border_strong)
        .text_size(px(12.5))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text_muted)
        .child(SharedString::from(name.to_string()))
}

fn stat(theme: &Theme, label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(section_label(theme, label))
        .child(
            div()
                .text_size(px(14.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(SharedString::from(value)),
        )
}

fn footer(
    theme: &Theme,
    run: Option<&RunHandle>,
    running: bool,
    error: Option<&str>,
    cx: &mut Context<ClickloopWindow>,
) -> impl IntoElement {
    let (cycles, elapsed_secs, reason) = match run {
        Some(run) => (
            run.progress.cycles(),
            run.progress.elapsed().as_secs_f64(),
            run.progress.reason(),
        ),
        None => (0, 0.0, None),
    };

    // One status line: an error outranks a run report, which outranks the idle text.
    let (status, status_color) = match (error, &reason) {
        (Some(error), _) => (error.to_string(), theme.danger),
        (None, Some(reason)) => (format!("Stopped: {reason}"), theme.text_muted),
        (None, None) if running => (
            "Working. Press the stop key to end it.".to_string(),
            theme.text_muted,
        ),
        (None, None) => ("Ready when you are".to_string(), theme.text_muted),
    };

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
                        .flex_1()
                        .text_size(px(11.5))
                        .text_color(status_color)
                        .child(SharedString::from(status)),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme.text_faint)
                                .child(SharedString::from(format!("{cycles} cycles"))),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme.text_faint)
                                .child(SharedString::from(format!("{elapsed_secs:.1}s"))),
                        ),
                ),
        )
        .child(
            div()
                .id("run-button")
                .w_full()
                .h(px(46.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RADIUS_CARD))
                .bg(if running { theme.stop } else { theme.accent })
                .text_color(if running { theme.text } else { theme.on_accent })
                .text_size(px(14.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .cursor_pointer()
                .hover_style(move |style| {
                    style.bg(if running {
                        theme.stop_hover
                    } else {
                        theme.accent_hover
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        if this.is_running() {
                            this.on_stop(cx);
                        } else {
                            this.on_run(cx);
                        }
                    }),
                )
                .child(if running { "Stop" } else { "Run" }),
        )
}

/// Seconds without a pointless trailing zero.
fn format_secs(value: f64) -> String {
    if (value.fract()).abs() < 1e-9 {
        format!("{}s", value as i64)
    } else {
        format!("{value:.2}s")
    }
}
