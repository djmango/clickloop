//! clickloop-gui: a small window over clickloop.

mod hotkey;
mod state;
mod store;
mod theme;
mod view;

use gpui::prelude::*;
use gpui::{App, Bounds, WindowBounds, WindowOptions, div, px, size};
use hotkey::Router;
use view::ClickloopWindow;

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.activate(true);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(
            None,
            size(px(theme::WINDOW_WIDTH), px(theme::WINDOW_HEIGHT)),
            cx,
        );
        cx.open_window(
            WindowOptions::new()
                .window_bounds(Some(WindowBounds::Windowed(bounds)))
                .titlebar(Some(gpui::TitlebarOptions {
                    title: Some("clickloop".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                })),
            |window, cx| {
                window.set_background_appearance(gpui::WindowBackgroundAppearance::Opaque);
                // One global listener for the life of the app, so the hotkeys
                // work whether or not this window has focus.
                // Bindings come from disk, so a rebind survives a restart.
                let router = Router::start(&store::load_bindings());
                cx.new(|cx| ClickloopWindow::new(router, cx))
            },
        )
        .expect("failed to open the clickloop window");
    });
}

/// Kept so the binary always links the panel body type.
#[allow(dead_code)]
fn _filler() -> impl IntoElement {
    div().size_full()
}
