//! The window's spacing, radii and type, in px. A Windows 11 window is 8 round and nothing inside
//! it is rounder: cards and groups 8, controls 4. See design/brand/README.md, "Shape and
//! spacing"; the agent keeps the same rules, at its own radii, in Dimens.kt.

/// From the window's edge to the content, and the card's padding.
pub const MARGIN: f32 = 16.;
pub const CARD_PADDING: f32 = 16.;
/// Between lines that belong together, such as a title and its sentence.
pub const TIGHT: f32 = 4.;
/// Between groups of content.
pub const GAP: f32 = 16.;
/// Above a section's label.
pub const SECTION_GAP: f32 = 20.;

pub const CARD_RADIUS: f32 = 8.;
pub const CONTROL_RADIUS: f32 = 4.;
/// The card's field and buttons. Set explicitly: the component kit's large field is 44 tall and
/// its large button 32.
pub const CONTROL_HEIGHT: f32 = 40.;
pub const ROW_HEIGHT: f32 = 52.;

/// Font size and line height.
pub struct Type(pub f32, pub f32);

pub const TITLE: Type = Type(26., 32.);
pub const FACT: Type = Type(16., 22.);
pub const ROW: Type = Type(14., 20.);
pub const SENTENCE: Type = Type(14., 20.);
pub const LABEL: Type = Type(13., 18.);
pub const FACT_LABEL: Type = Type(12., 16.);
