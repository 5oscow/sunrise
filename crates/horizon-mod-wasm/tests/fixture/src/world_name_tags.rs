//! The shared native/wasm `WorldDrawContext::name_tag` scene (#4171). It draws only at
//! `AfterLevel` in a view whose game time is `SCENE_TIME`, so other tests' views stay empty.
use mod_api::{NameTagOptions, RenderLevelStage, RenderLevelStageEvent, Text, text::TextStyle};

/// The game time of the one view this scene draws in.
pub const SCENE_TIME: i64 = 4171;

/// The scene's labels: one of each kind the review's offscreen tests pin, then one refused.
pub fn scene() -> Vec<([f64; 3], Text, NameTagOptions)> {
    let mut bold = TextStyle::default();
    bold.bold = Some(true);
    vec![
        // Hidden behind a wall: one translucent copy, in light 0 as given.
        (
            [0.5, 70.0, 0.5],
            Text::literal("Hidden"),
            NameTagOptions::new(false, true, 0x00),
        ),
        // See-through in light 0: the copy in the world at light 2, the faded one unlit.
        (
            [-1.0, 70.3, 1.5],
            Text::literal("Through"),
            NameTagOptions::new(true, true, 0x00),
        ),
        // Styles counted in the centring width; no backdrop; full sky light.
        (
            [1.5, 69.6, 2.5],
            Text::literal("Bold").with_style(bold),
            NameTagOptions::new(false, false, 0xF0),
        ),
        // Refused by name; draws nothing.
        (
            [f64::NAN, 70.0, 0.5],
            Text::literal("NaN"),
            NameTagOptions::new(true, true, 0xF0),
        ),
    ]
}

pub fn render_level_stage(event: &mut RenderLevelStageEvent<'_>) {
    if event.stage != RenderLevelStage::AfterLevel || event.view.game_time != SCENE_TIME {
        return;
    }
    for (at, text, options) in scene() {
        let _ = event.draw.name_tag(at, &text, options);
    }
}
