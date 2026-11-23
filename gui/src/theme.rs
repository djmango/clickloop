//! The palette, in one place, so the window reads as one design.
//!
//! A cool graphite ground with one blue accent. Every colour is a token, used
//! everywhere it appears, so there is nowhere for a second style to creep in.

use gpui::Rgba;

pub struct Theme {
    /// The window ground.
    pub bg: Rgba,
    /// A raised surface: the field, cards, chips.
    pub surface: Rgba,
    /// A surface under the pointer.
    pub surface_hover: Rgba,
    /// A hairline around a surface.
    pub line: Rgba,
    /// A stronger hairline, for a keycap or a selected chip.
    pub line_strong: Rgba,
    /// Body text.
    pub text: Rgba,
    /// Secondary text: values, hints.
    pub text_dim: Rgba,
    /// Labels and captions.
    pub text_faint: Rgba,
    /// The one accent: buttons, progress, focus, selection.
    pub accent: Rgba,
    pub accent_hover: Rgba,
    /// The accent at low strength, for a selected chip.
    pub accent_soft: Rgba,
    /// Text drawn on the accent.
    pub on_accent: Rgba,
    /// A warning, for a countdown.
    pub warn: Rgba,
    /// A problem, for an invalid sequence.
    pub danger: Rgba,
    /// The unfilled part of the progress track.
    pub track: Rgba,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: rgb(0x15181f),
            surface: rgb(0x1e232d),
            surface_hover: rgb(0x272d39),
            line: rgb(0x2d3441),
            line_strong: rgb(0x3d4554),
            text: rgb(0xf2f4f8),
            text_dim: rgb(0xb9c1cf),
            text_faint: rgb(0x8d97a8),
            accent: rgb(0x4c8dff),
            accent_hover: rgb(0x6ba0ff),
            accent_soft: rgb(0x1e2c44),
            on_accent: rgb(0x0b1220),
            warn: rgb(0xf0b64a),
            danger: rgb(0xff7d72),
            track: rgb(0x3d4554),
        }
    }
}

use gpui::rgb;

pub const RADIUS_CARD: f32 = 10.0;
pub const RADIUS_CHIP: f32 = 7.0;
pub const RADIUS_KEYCAP: f32 = 5.0;

/// The window the layout is designed for. Its height is measured from the
/// render: content plus the bottom padding, so nothing is ever clipped.
pub const WINDOW_WIDTH: f32 = 440.0;
pub const WINDOW_HEIGHT: f32 = 392.0;

/// The same colour with a different opacity.
pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    use gpui::ColorExt as _;
    color.opacity(alpha)
}
