//! A label's lines, as `DrawContext::text_lines` measures and places them: no widget, no theme, no
//! state, so a mod draws each line itself.
use crate::Text;

/// Where a label's lines sit inside its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// exhaustive: a closed set the game matches without a wildcard
#[derive(sunrise_mod_api_macros::Wire)]
pub enum TextAlign {
    /// Lined up on the left edge of the width.
    Left,
    /// Centred in the width: `(width - line width) / 2` from its left edge.
    Centre,
    /// Lined up on the right edge of the width.
    Right,
}

/// One line of a label: the piece of the component it carries, and where to draw it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
#[derive(sunrise_mod_api_macros::Wire)]
pub struct TextLine {
    /// The component's characters this line holds, styles and colours kept.
    pub text: Text,
    /// Where to draw it: `x` from the label's left edge, `y` from its top (`0` for the first line).
    pub at: [f32; 2],
    /// Its width in GUI pixels, as `Font.width` measures it, styles included.
    pub width: f32,
}

impl TextLine {
    /// A line `width` GUI pixels wide, drawn with its top left at `at`.
    pub fn new(text: Text, at: [f32; 2], width: f32) -> Self {
        Self { text, at, width }
    }
}

/// A label laid out: its lines, already placed inside its width, and the block it occupies.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
#[derive(sunrise_mod_api_macros::Wire)]
pub struct TextLines {
    /// The lines, top to bottom.
    pub lines: Vec<TextLine>,
    /// The widest line's width in GUI pixels, `0` with no line.
    pub width: f32,
    /// The block's height in GUI pixels: the last line's `at[1]` plus one line height, `0` with no
    /// line. What a scroll container sizes its content from.
    pub height: f32,
}

impl TextLines {
    /// The layout a draw answers: `lines` already placed, the widest line's width and the block's
    /// height.
    pub fn new(lines: Vec<TextLine>, width: f32, height: f32) -> Self {
        Self {
            lines,
            width,
            height,
        }
    }
}
