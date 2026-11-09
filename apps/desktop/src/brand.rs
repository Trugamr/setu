//! Setu's brand in the window: the colours, the theme built from them, and the mark.
//! See `design/brand/README.md`.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

pub fn ink() -> Hsla {
    rgb(0x15141A).into()
}

pub fn cream() -> Hsla {
    rgb(0xF5ECDC).into()
}

pub fn signal() -> Hsla {
    rgb(0xFF3B2F).into()
}

// Steps up from Ink for raised surfaces, lines and secondary text, as in the agent's theme.
pub fn ink_raised() -> Hsla {
    rgb(0x1F1E25).into()
}

pub fn ink_high() -> Hsla {
    rgb(0x2A2930).into()
}

pub fn line() -> Hsla {
    rgb(0x4A4851).into()
}

pub fn cream_muted() -> Hsla {
    rgb(0xA9A398).into()
}

fn signal_hover() -> Hsla {
    rgb(0xFF5A4F).into()
}

fn signal_active() -> Hsla {
    rgb(0xE5352A).into()
}

/// Recolours the component theme in the brand: dark only, with Signal as the one accent.
/// Text on Signal is Ink, since Cream on red falls short of 4.5:1 contrast.
pub fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    // `update` carries colour edits through to the tokens the components paint with; switching
    // mode reloads the registered theme, so the edits come after `change`.
    Theme::update(cx, |theme| {
        let c = &mut theme.colors;

        c.background = ink();
        c.foreground = cream();
        c.border = line();
        c.input = line();
        c.ring = signal();
        c.caret = signal();
        c.selection = signal().opacity(0.35);
        c.title_bar = ink();
        c.window_border = line();

        c.primary = signal();
        c.primary_hover = signal_hover();
        c.primary_active = signal_active();
        c.primary_foreground = ink();
        c.button_primary = signal();
        c.button_primary_hover = signal_hover();
        c.button_primary_active = signal_active();
        c.button_primary_foreground = ink();

        c.secondary = ink_high();
        c.secondary_hover = line();
        c.secondary_active = line();
        c.secondary_foreground = cream();
        c.button = ink_raised();
        c.button_hover = ink_high();
        c.button_active = line();
        c.button_foreground = cream();

        c.muted = ink_high();
        c.muted_foreground = cream_muted();
        c.accent = ink_high();
        c.accent_foreground = cream();
        c.popover = ink_raised();
        c.popover_foreground = cream();
        c.list = ink_raised();
        c.list_hover = ink_high();
        c.list_active = ink_high();
        c.list_active_border = signal();
        c.switch = ink_high();
        c.switch_thumb = cream();
        c.danger = signal();
        c.danger_foreground = ink();
    });
}

/// The mark's dots in its 100-unit tile: a 7 × 5 grille on a 9-unit grid, an arch of radius 14.4
/// at (50, 72.5) cut from the bottom, and the two top corners dropped.
fn mark_dots() -> impl Iterator<Item = (f32, f32)> {
    (0..5)
        .flat_map(|row| (0..7).map(move |col| (23.0 + 9.0 * col as f32, 32.0 + 9.0 * row as f32)))
        .filter(|&(x, y)| ((x - 50.0).powi(2) + (y - 72.5).powi(2)).sqrt() >= 14.4)
        .filter(|&(x, y)| !(y == 32.0 && (x == 23.0 || x == 77.0)))
}

/// The red power light.
const LIGHT: (f32, f32) = (68.0, 32.0);

/// The mark without its tile, cropped to the dots, `width` wide.
pub fn mark(width: f32) -> impl IntoElement {
    // The dots span x 20.1 to 79.9 and y 29.1 to 70.9 in tile units.
    let scale = width / 59.8;
    let mut el = div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(41.8 * scale));
    for (x, y) in mark_dots() {
        el = el.child(
            div()
                .absolute()
                .left(px((x - 23.0) * scale))
                .top(px((y - 32.0) * scale))
                .size(px(5.8 * scale))
                .rounded_full()
                .bg(if (x, y) == LIGHT { signal() } else { cream() }),
        );
    }
    el
}

/// A row of grille dots as a level meter: `level` (0 to 1) of them lit in Cream, with the top
/// two in Signal once lit.
pub fn meter(level: f32, dots: usize) -> impl IntoElement {
    let lit = (level * dots as f32).round() as usize;
    let mut row = div().flex().gap(px(7.));
    for i in 0..dots {
        let color = match i {
            i if i >= lit => ink_high(),
            i if i + 2 >= dots => signal(),
            _ => cream(),
        };
        row = row.child(div().size(px(9.)).rounded_full().bg(color));
    }
    row
}
