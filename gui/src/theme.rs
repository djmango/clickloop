//! The palette, in one place, so the window reads as one design.

use gpui::{Pixels, Rgba, px, rgb};

/// A colour token: one definition, used everywhere it appears.
pub struct Theme {
    pub bg: Rgba,
    pub panel: Rgba,
    pub panel_hover: Rgba,
    pub border: Rgba,
    pub border_strong: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    pub text_faint: Rgba,
    pub accent: Rgba,
    pub accent_hover: Rgba,
    pub accent_press: Rgba,
    pub on_accent: Rgba,
    pub run: Rgba,
    pub run_hover: Rgba,
    pub stop: Rgba,
    pub stop_hover: Rgba,
    pub danger: Rgba,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: rgb(0x0c0d10),
            panel: rgb(0x15171c),
            panel_hover: rgb(0x1c1f26),
            border: rgb(0x24272f),
            border_strong: rgb(0x333843),
            text: rgb(0xf4f5f7),
            text_muted: rgb(0xb3bac6),
            text_faint: rgb(0x8b93a1),
            accent: rgb(0xc8f751),
            accent_hover: rgb(0xd6ff6e),
            accent_press: rgb(0xb4e03c),
            on_accent: rgb(0x12140f),
            run: rgb(0xc8f751),
            run_hover: rgb(0xd6ff6e),
            stop: rgb(0x2a2e37),
            stop_hover: rgb(0x363b47),
            danger: rgb(0xff7a6b),
        }
    }
}

/// Spacing scale, so paddings agree.
pub const GAP_XS: Pixels = px(6.0);
pub const GAP_SM: Pixels = px(10.0);
pub const GAP_MD: Pixels = px(16.0);
pub const GAP_LG: Pixels = px(24.0);
pub const GAP_XL: Pixels = px(32.0);

pub const RADIUS_SM: Pixels = px(8.0);
pub const RADIUS_MD: Pixels = px(12.0);
pub const RADIUS_LG: Pixels = px(16.0);

/// The window size the layout is designed for.
pub const WINDOW_WIDTH: f32 = 460.0;
pub const WINDOW_HEIGHT: f32 = 424.0;

/// The same colour with a different opacity.
pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    use gpui::ColorExt as _;
    color.opacity(alpha)
}
