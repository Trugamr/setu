//! Setu's brand in the window: the component theme built from the palette, and the grille.
//! See `design/brand/README.md`.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use setu::view::Look;

use crate::dimens;
use crate::palette::*;

/// A palette token as a GPUI colour.
pub fn hsla(token: u32) -> Hsla {
    rgb(token).into()
}

/// Recolours the component theme in the brand: dark only, Signal as the one accent, Edge
/// outlining every control, Cream for focus. Text on Signal is Ink, since Cream on red is 3:1.
/// Controls are 4 round, popovers 8, as in [`dimens`].
pub fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    // `update` carries colour edits through to the tokens the components paint with; switching
    // mode reloads the registered theme, so the edits come after `change`.
    Theme::update(cx, |theme| {
        theme.radius = px(dimens::CONTROL_RADIUS);
        theme.radius_lg = px(dimens::CARD_RADIUS);
        // A focused control shows a 1px border in `ring`, not the kit's soft band around it.
        theme.focus_ring = false;
        let c = &mut theme.colors;

        c.background = hsla(INK);
        c.foreground = hsla(CREAM);
        // Popover and divider outlines are decoration; the outlines that identify a control
        // (inputs, selects, outline buttons) are `input`, in Edge.
        c.border = hsla(LINE);
        c.input = hsla(EDGE);
        // Focus is Cream: a field being typed in must not look like an error.
        c.ring = hsla(CREAM);
        c.caret = hsla(CREAM);
        c.selection = hsla(CREAM).opacity(0.25);
        c.title_bar = hsla(INK);
        c.window_border = hsla(LINE);

        c.primary = hsla(SIGNAL);
        c.primary_hover = hsla(SIGNAL_HOVER);
        c.primary_active = hsla(SIGNAL_PRESSED);
        c.primary_foreground = hsla(INK);
        c.button_primary = hsla(SIGNAL);
        c.button_primary_hover = hsla(SIGNAL_HOVER);
        c.button_primary_active = hsla(SIGNAL_PRESSED);
        c.button_primary_foreground = hsla(INK);

        c.secondary = hsla(INK_HIGH);
        c.secondary_hover = hsla(INK_HOVER);
        c.secondary_active = hsla(UNLIT);
        c.secondary_foreground = hsla(CREAM);
        c.button = hsla(INK_RAISED);
        c.button_hover = hsla(INK_HIGH);
        c.button_active = hsla(INK_HOVER);
        c.button_foreground = hsla(CREAM);

        c.muted = hsla(INK_HIGH);
        c.muted_foreground = hsla(CREAM_MUTED);
        // Hover and selection inside menus, which sit on InkRaised.
        c.accent = hsla(INK_HOVER);
        c.accent_foreground = hsla(CREAM);
        c.popover = hsla(INK_RAISED);
        c.popover_foreground = hsla(CREAM);
        c.list = hsla(INK_RAISED);
        c.list_hover = hsla(INK_HOVER);
        c.list_active = hsla(INK_HOVER);
        c.list_active_border = hsla(CREAM);
        c.danger = hsla(SIGNAL);
        c.danger_foreground = hsla(INK);
    });
}

/// The grille from the icon, five dots by three, as tall as the status title's line, as a level
/// meter: `level` (0 to 1) of the dots lit, column by column from the bottom left. On the Cream
/// card they are Ink over a faint Ink scale; elsewhere every dot is Unlit, since nothing plays.
pub fn grille(look: Look, level: f32) -> impl IntoElement {
    const COLUMNS: usize = 5;
    const ROWS: usize = 3;
    let lit = match look {
        Look::Flowing => (level * (COLUMNS * ROWS) as f32).round() as usize,
        _ => 0,
    };
    let (on, off) = match look {
        Look::Flowing => (hsla(INK), hsla(INK).opacity(0.2)),
        _ => (hsla(CREAM), hsla(UNLIT)),
    };
    let mut grille = div().flex().flex_none().gap(px(4.));
    for col in 0..COLUMNS {
        let mut column = div().flex().flex_col().gap(px(4.));
        for row in 0..ROWS {
            let index = col * ROWS + (ROWS - 1 - row);
            column = column.child(div().size(px(8.)).rounded_full().bg(if index < lit {
                on
            } else {
                off
            }));
        }
        grille = grille.child(column);
    }
    grille
}
