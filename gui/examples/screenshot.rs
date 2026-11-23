//! Renders the real window off-screen and saves a PNG.
//!
//! This drives `ClickloopWindow` itself, not a copy of it, so the picture
//! cannot drift from the panel the app shows.
//!
//!     cargo run --example screenshot --features visual-test -- out.png [idle|running|stopped|error|history|reference]

#[cfg(not(target_os = "macos"))]
use gpui::HeadlessAppContext as VisualContext;
#[cfg(target_os = "macos")]
use gpui::VisualTestAppContext as VisualContext;

#[allow(dead_code)]
#[path = "../src/hotkey.rs"]
mod hotkey;
#[allow(dead_code)]
#[path = "../src/state.rs"]
mod state;
#[allow(dead_code)]
#[path = "../src/store.rs"]
mod store;
#[allow(dead_code)]
#[path = "../src/theme.rs"]
mod theme;
#[allow(dead_code)]
#[path = "../src/view.rs"]
mod view;

use gpui::prelude::*;
use gpui::{AppContext as _, IntoElement, Render, Window, div, px, size};
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "screenshot.png".to_string());
    let phase_name = args.get(2).cloned().unwrap_or_else(|| "idle".to_string());

    let mut form = state::Form::default();
    let bindings = hotkey::Bindings::default();
    let history: Vec<String> = vec![
        "1,1,click".to_string(),
        "(1,1,click)*5".to_string(),
        "space*3,click".to_string(),
        "cmd+s".to_string(),
        "wait:0.5,click".to_string(),
    ];
    let mut panel = view::Panel::None;

    let pinned = match phase_name.as_str() {
        "running" => {
            form.actions = "(1,1,click)*5".to_string();
            form.delay = 0.2;
            form.duration = 30.0;
            Some((
                state::Phase::Running,
                148u64,
                Duration::from_millis(10200),
                Some((437u64, Duration::from_secs(30))),
                None,
                None,
            ))
        }
        "stopped" => {
            form.actions = "(1,1,click)*5".to_string();
            form.delay = 0.2;
            form.duration = 30.0;
            Some((
                state::Phase::Stopped,
                437,
                Duration::from_secs(30),
                Some((437, Duration::from_secs(30))),
                Some("cycle count reached".to_string()),
                None,
            ))
        }
        "error" => {
            form.actions = "cmd+shif".to_string();
            Some((
                state::Phase::Idle,
                0,
                Duration::ZERO,
                None,
                None,
                Some("\"cmd+shif\" is not a key name".to_string()),
            ))
        }
        "history" => {
            panel = view::Panel::History;
            None
        }
        "reference" => {
            panel = view::Panel::Reference;
            None
        }
        _ => None,
    };

    let window = match pinned {
        Some((phase, cycles, elapsed, target, reason, error)) => view::ClickloopWindow::previewed(
            form, phase, cycles, elapsed, target, reason, error, bindings, history, panel,
        ),
        None => view::ClickloopWindow::previewed(
            form,
            state::Phase::Idle,
            0,
            Duration::ZERO,
            None,
            None,
            None,
            bindings,
            history,
            panel,
        ),
    };

    #[cfg(target_os = "macos")]
    let mut cx = VisualContext::new(gpui_platform::current_platform(false));

    #[cfg(not(target_os = "macos"))]
    let mut cx = {
        let platform = gpui_platform::current_platform(true);
        let text_system = platform.text_system();
        VisualContext::with_platform(text_system, std::sync::Arc::new(()), || {
            Some(Box::new(gpui_wgpu::WgpuHeadlessRenderer::new().ok()?)
                as Box<dyn gpui::PlatformHeadlessRenderer>)
        })
    };

    let width = px(theme::WINDOW_WIDTH);
    let height = px(theme::WINDOW_HEIGHT);

    #[cfg(target_os = "macos")]
    let handle = cx
        .open_offscreen_window(size(width, height), |_, cx| cx.new(|_| window))
        .expect("failed to open the offscreen window");

    #[cfg(not(target_os = "macos"))]
    let handle = cx
        .open_window(size(width, height), |_, cx| cx.new(|_| window))
        .expect("failed to open the headless window");

    let handle = handle.into();
    cx.run_until_parked();

    let image = cx
        .capture_screenshot(handle)
        .expect("failed to capture the window");
    image.save(&out).expect("failed to save the screenshot");
    println!("saved {} ({}x{})", out, image.width(), image.height());
}
