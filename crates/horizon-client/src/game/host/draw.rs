//! The game's side of the `draw` area (`horizon-mod-api/src/host/draw.rs`).
use crate::game::{
    Game,
    api_values::{TryIntoApi, TryIntoEngine},
};
use mod_api::{prelude::*, *};

/// [`DrawContext`] on the game's `crate::hud::HudBatch`: one per hook call.
pub(crate) struct GameDrawContext<'a, 'g> {
    pub(crate) game: &'a Game<'g>,
    pub(crate) surface: &'a mut crate::hud::HudBatch,
    /// The call's open clips (`push_clip`), closed when the call returns.
    clips: crate::game::gui_clips::GuiClips,
}
impl<'a, 'g> GameDrawContext<'a, 'g> {
    pub(crate) fn new(game: &'a Game<'g>, surface: &'a mut crate::hud::HudBatch) -> Self {
        Self {
            game,
            surface,
            clips: Default::default(),
        }
    }
}
impl Drop for GameDrawContext<'_, '_> {
    fn drop(&mut self) {
        self.clips.close_all(self.surface);
    }
}
impl DrawContext for GameDrawContext<'_, '_> {
    fn size(&self) -> [f32; 2] {
        self.game.native_size(&*self.surface)
    }
    fn item_model(&mut self, item: ItemKey, model: &str, at: [f32; 2], size: f32) -> bool {
        let Some(item) = mod_api::__private::item_key_id(item) else {
            return false;
        };
        self.game
            .native_item_model(&mut *self.surface, item, model, at, size)
    }
    fn quad(&mut self, points: [[f32; 2]; 4], argb: u32) {
        self.game.native_quad(&mut *self.surface, points, argb)
    }
    fn fill(&mut self, rect: [f32; 4], argb: u32) {
        self.game.native_fill(&mut *self.surface, rect, argb)
    }
    fn fill_gradient(&mut self, rect: [f32; 4], top: u32, bottom: u32) {
        self.game
            .native_fill_gradient(&mut *self.surface, rect, top, bottom)
    }
    fn sprite(&mut self, name: &str, src: [f32; 4], dst: [f32; 4], tint: Option<u32>) -> bool {
        self.game
            .native_sprite(&mut *self.surface, name, src, dst, tint)
    }
    fn item(&mut self, item: ItemKey, at: [f32; 2], size: f32) -> bool {
        let Some(item) = mod_api::__private::item_key_id(item) else {
            return false;
        };
        self.game.native_item(&mut *self.surface, item, at, size)
    }
    fn text(&mut self, text: &str, at: [f32; 2], argb: u32, shadow: bool) {
        self.game
            .native_text(&mut *self.surface, text, at, argb, shadow)
    }
    fn item_stack(&mut self, stack: &ItemStack, at: [f32; 2], size: f32) -> bool {
        let Ok(stack) = crate::game::api_values::stack_value(stack) else {
            return false;
        };
        self.game
            .native_item_stack(&mut *self.surface, &stack, at, size)
    }
    fn gui_paint(&mut self, draws: &[GuiDraw]) -> Result<(), mod_api::text::TextFormError> {
        let draws = draws.try_into_engine(&())?;
        self.game.native_gui_paint(&mut *self.surface, &draws);
        Ok(())
    }
    fn gui_nine_slice(&mut self, sprite: &str, rect: [f32; 4], color: u32) {
        self.game
            .native_gui_nine_slice(&mut *self.surface, sprite, rect, color)
    }
    fn gui_tooltip(
        &mut self,
        lines: &[Text],
        at: [f32; 2],
    ) -> Result<(), mod_api::text::TextFormError> {
        let lines = lines.try_into_engine(&())?;
        self.game.native_gui_tooltip(&mut *self.surface, &lines, at);
        Ok(())
    }
    fn text_width(&self, text: &str) -> f32 {
        self.game.native_text_width(&*self.surface, text)
    }
    fn text_lines(
        &self,
        text: &Text,
        max_width: f32,
        align: TextAlign,
        line_spacing: f32,
    ) -> Result<TextLines, mod_api::text::TextFormError> {
        let text: crate::text::Text = text.try_into_engine(&())?;
        label_layout(
            &self.game.gui.atlas.font,
            &text,
            max_width,
            align,
            line_spacing,
        )
    }
    fn fluid(&mut self, fluid: &str, at: [f32; 2], size: f32) {
        self.game.native_fluid(&mut *self.surface, fluid, at, size)
    }
    fn draw_scene(
        &mut self,
        rect: [f32; 4],
        scene: &mod_api::Scene,
        view: SceneView,
    ) -> Result<(), mod_api::SceneError> {
        let blocks = crate::game::api_values::aggregates::scene_blocks(&scene.blocks)?;
        self.game
            .native_draw_scene(&mut *self.surface, rect, &blocks, scene, view)
    }
    fn push_clip(&mut self, rect: [f32; 4]) -> Result<(), GuiClipRefusal> {
        self.clips.push(self.surface, rect)
    }
    fn pop_clip(&mut self) -> Result<(), GuiClipRefusal> {
        self.clips.pop(self.surface)
    }
    fn entity(&mut self, entity: &GuiEntity, view: GuiEntityView) -> Result<(), GuiEntityError> {
        self.game
            .native_draw_entity(&mut *self.surface, entity, view)
    }
    fn draw_menu_background(&mut self, rect: [f32; 4], in_world: bool) {
        self.game
            .native_draw_menu_background(&mut *self.surface, rect, in_world)
    }
    fn tiled_sprite(&mut self, sprite: &str, rect: [f32; 4], size: f32, argb: u32) -> bool {
        self.game
            .native_tiled_sprite(&mut *self.surface, sprite, rect, size, argb)
    }
    fn gui_draw_item_with(&mut self, stack: &ItemStack, at: [f32; 2], text: Option<&str>) {
        let Ok(stack) = crate::game::api_values::stack_value(stack) else {
            return;
        };
        self.game
            .native_gui_draw_item_with(&mut *self.surface, &stack, at, text)
    }
    fn widget_button(
        &mut self,
        rect: [f32; 4],
        label: &Text,
        active: bool,
        highlighted: bool,
    ) -> Result<(), mod_api::text::TextFormError> {
        let label = label.try_into_engine(&())?;
        self.game
            .native_widget_button(&mut *self.surface, rect, &label, active, highlighted);
        Ok(())
    }
    fn widget_text_field(&mut self, field: &TextFieldView) -> TextFieldLayout {
        self.game
            .native_widget_text_field(&mut *self.surface, field)
    }
}
/// `Font.lineHeight`: nine GUI pixels a line.
const LINE_HEIGHT: f32 = 9.0;
/// Lays `text` out as a label's lines, as [`DrawContext::text_lines`] documents: the font's own
/// split wraps it, the font measures each line, and `align` places them inside `max_width`.
fn label_layout(
    font: &crate::font::Font,
    text: &crate::text::Text,
    max_width: f32,
    align: TextAlign,
    line_spacing: f32,
) -> Result<TextLines, mod_api::text::TextFormError> {
    // A spacing that is not a number cannot place anything: no spacing, rather than every line at
    // an undefined height.
    let line_spacing = if line_spacing.is_finite() {
        line_spacing
    } else {
        0.0
    };
    // No width to wrap in, and so none to align in: one line per explicit newline, each at x 0.
    let wraps = max_width.is_finite() && max_width > 0.0;
    let wrap_width = if wraps { max_width } else { f32::INFINITY };
    let wrapped = font.split_formatted(&text.resolved(), wrap_width);
    let widths: Vec<f32> = wrapped.iter().map(|line| font.width_text(line)).collect();
    let width = widths.iter().copied().fold(0.0, f32::max);
    let mut lines = Vec::with_capacity(wrapped.len());
    for (i, (text, width)) in wrapped.into_iter().zip(widths).enumerate() {
        let x = if wraps {
            match align {
                TextAlign::Left => 0.0,
                TextAlign::Centre => (max_width - width) / 2.0,
                TextAlign::Right => max_width - width,
            }
        } else {
            0.0
        };
        let at = [x, i as f32 * (LINE_HEIGHT + line_spacing)];
        lines.push(TextLine::new(text.try_into_api()?, at, width));
    }
    let height = lines.last().map_or(0.0, |line| line.at[1] + LINE_HEIGHT);
    Ok(TextLines::new(lines, width, height))
}
/// [`BlockEntityDrawContext`] on the game's `crate::game::native_block_entity_draw::BlockEntityDraw`.
pub(crate) struct GameBlockEntityDrawContext<'a, 'g> {
    pub(crate) game: &'a Game<'g>,
    pub(crate) surface: &'a mut crate::game::native_block_entity_draw::BlockEntityDraw,
}
impl<'a, 'g> GameBlockEntityDrawContext<'a, 'g> {
    pub(crate) fn new(
        game: &'a Game<'g>,
        surface: &'a mut crate::game::native_block_entity_draw::BlockEntityDraw,
    ) -> Self {
        Self { game, surface }
    }
}
impl BlockEntityDrawContext for GameBlockEntityDrawContext<'_, '_> {
    fn push(&mut self) {
        self.game.native_draw_push(&mut *self.surface)
    }
    fn pop(&mut self) {
        self.game.native_draw_pop(&mut *self.surface)
    }
    fn translate(&mut self, by: [f32; 3]) {
        self.game.native_draw_translate(&mut *self.surface, by)
    }
    fn rotate_about(&mut self, axis: [f32; 3], degrees: f32, pivot: [f32; 3]) {
        self.game
            .native_draw_rotate_about(&mut *self.surface, axis, degrees, pivot)
    }
    fn scale(&mut self, by: [f32; 3]) {
        self.game.native_draw_scale(&mut *self.surface, by)
    }
    fn keep(&mut self) {
        self.game.native_draw_keep(&mut *self.surface)
    }
    fn partial(&self) -> f32 {
        self.game.native_draw_partial(&*self.surface)
    }
    fn game_time(&self) -> i64 {
        self.game.native_draw_game_time(&*self.surface)
    }
    fn animation_time(&self) -> f64 {
        self.game.native_draw_animation_time(&*self.surface)
    }
    fn light(&self) -> u32 {
        self.game.native_draw_light(&*self.surface)
    }
    fn set_light(&mut self, light: u32) {
        self.game.native_draw_set_light(&mut *self.surface, light)
    }
    fn quad(&mut self, sprite: &str, corners: [[f32; 3]; 4], uv: [f32; 4], tint: u32) -> bool {
        self.game
            .native_draw_quad(&mut *self.surface, sprite, corners, uv, tint)
    }
    fn cube(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        faces: [Option<CubeFace<'_>>; 6],
        tint: u32,
    ) -> bool {
        self.game
            .native_draw_cube(&mut *self.surface, from, to, faces, tint)
    }
    fn block(&mut self, block: BlockKey, properties: &[(&str, &str)]) -> bool {
        let Some(block) = mod_api::__private::block_key_id(block) else {
            return false;
        };
        self.game
            .native_draw_block(&mut *self.surface, block, properties)
    }
    fn item(
        &mut self,
        stack: &ItemStack,
        display: ItemDisplayContext,
        owner_yaw: Option<f32>,
        seed: i32,
    ) -> bool {
        let Ok(stack) = crate::game::api_values::stack_value(stack) else {
            return false;
        };
        let Ok(display) = display.try_into_engine(self.game.items()) else {
            return false;
        };
        self.game
            .native_draw_item(&mut *self.surface, &stack, display, owner_yaw, seed)
    }
    fn spin(&mut self, axis: [f32; 3], pivot: [f32; 3], degrees_per_tick: f32, phase: f32) {
        self.game
            .native_draw_spin(&mut *self.surface, axis, pivot, degrees_per_tick, phase)
    }
    fn still(&mut self) {
        self.game.native_draw_still(&mut *self.surface)
    }
    fn model(&mut self, model: ModelRef, tint: u32) -> Result<(), ModelError> {
        self.game.native_draw_model(&mut *self.surface, model, tint)
    }
    fn text(
        &mut self,
        text: &Text,
        at: [f32; 2],
        color: u32,
        glowing: bool,
        outline: Option<u32>,
    ) -> Result<f32, mod_api::text::TextFormError> {
        let text = text.try_into_engine(&())?;
        Ok(self
            .game
            .native_draw_text(&mut *self.surface, &text, at, color, glowing, outline))
    }
    fn terrain_model(
        &mut self,
        model: ModelRef,
        options: TerrainModelOptions<'_>,
    ) -> Result<(), TerrainModelError> {
        self.game
            .native_draw_terrain_model(&mut *self.surface, model, options)
    }
    fn text_width(&self, text: &Text) -> Result<f32, mod_api::text::TextFormError> {
        let text = text.try_into_engine(&())?;
        Ok(self.game.native_draw_text_width(&*self.surface, &text))
    }
    fn model_part(&mut self, part: &ModelPart, sprite: &str, tint: u32) -> bool {
        self.game
            .native_draw_model_part(&mut *self.surface, part, sprite, tint)
    }
    fn fluid_box(
        &mut self,
        stack: &FluidStack,
        bounds: [f32; 6],
        options: FluidBoxOptions,
    ) -> Result<(), FluidBoxError> {
        self.game
            .native_draw_fluid_box(&mut *self.surface, stack, bounds, options)
    }
}
/// [`CameraDrawContext`] on the game's `crate::screen_effects::CameraQuads`.
pub(crate) struct GameCameraDrawContext<'a, 'g> {
    pub(crate) game: &'a Game<'g>,
    pub(crate) surface: &'a mut crate::screen_effects::CameraQuads,
}
impl<'a, 'g> GameCameraDrawContext<'a, 'g> {
    pub(crate) fn new(
        game: &'a Game<'g>,
        surface: &'a mut crate::screen_effects::CameraQuads,
    ) -> Self {
        Self { game, surface }
    }
}
impl CameraDrawContext for GameCameraDrawContext<'_, '_> {
    fn quad(&mut self, corners: [[f32; 3]; 4], argb: u32) -> bool {
        self.game
            .native_camera_quad(&mut *self.surface, corners, argb)
    }
    fn sprite(&mut self, name: &str, corners: [[f32; 3]; 4], uv: [[f32; 2]; 4], argb: u32) -> bool {
        self.game
            .native_camera_sprite(&mut *self.surface, name, corners, uv, argb)
    }
}
/// [`WorldDrawContext`] on the game's `crate::world_geometry::WorldGeometry`.
pub(crate) struct GameWorldDrawContext<'a, 'g> {
    pub(crate) game: &'a Game<'g>,
    pub(crate) surface: &'a mut crate::world_geometry::WorldGeometry,
}
impl<'a, 'g> GameWorldDrawContext<'a, 'g> {
    pub(crate) fn new(
        game: &'a Game<'g>,
        surface: &'a mut crate::world_geometry::WorldGeometry,
    ) -> Self {
        Self { game, surface }
    }
}
impl WorldDrawContext for GameWorldDrawContext<'_, '_> {
    fn draw_instance_group(&mut self, group: InstanceGroupRef) -> Result<(), GroupError> {
        self.game.draw_instance_group(self.surface, group)
    }
    fn lines(&mut self, lines: &[WorldLine]) -> Result<(), DrawError> {
        self.game.native_world_draw_lines(&mut *self.surface, lines)
    }
    fn quads(&mut self, quads: &[WorldQuad]) -> Result<(), DrawError> {
        self.game.native_world_draw_quads(&mut *self.surface, quads)
    }
    fn pipeline_quads(
        &mut self,
        pipeline: RenderPipelineRef,
        quads: &[PipelineQuad],
    ) -> Result<(), PipelineDrawError> {
        self.game
            .native_world_draw_pipeline_quads(&mut *self.surface, pipeline, quads)
    }
    fn usage(&self) -> WorldDrawUsage {
        self.game.native_world_draw_usage(&*self.surface)
    }
    fn draw_model_instances(
        &mut self,
        model: ModelRef,
        instances: &[ModelInstance],
    ) -> Result<(), InstanceError> {
        self.game
            .native_world_draw_model_instances(&mut *self.surface, model, instances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A font whose printable characters are all `advance` GUI pixels wide: the vanilla provider
    /// list with a `space` provider, so a layout test measures exact advances.
    fn fake_font(advance: f32) -> crate::font::Font {
        let advances: Vec<String> = (32u8..=126)
            .map(char::from)
            // A JSON object key: the quote and the backslash are the two that need escaping.
            .filter(|&c| !matches!(c, '"' | '\\'))
            .map(|c| format!("\"{c}\":{advance}"))
            .collect();
        let json = format!(
            r#"{{"providers":[{{"type":"space","advances":{{{}}}}}]}}"#,
            advances.join(",")
        );
        let source = crate::resource_source::ResourceSource::indexed(BTreeMap::from([(
            "assets/minecraft/font/default.json".into(),
            crate::resource_source::Resource::bytes(json.into_bytes()),
        )]));
        crate::font::Font::metrics(&source)
    }

    /// A line's component as the game draws it.
    fn engine(text: &mod_api::Text) -> crate::text::Text {
        text.try_into_engine(&()).unwrap()
    }

    /// The lines' plain characters, top to bottom.
    fn plain(lines: &TextLines) -> Vec<String> {
        lines
            .lines
            .iter()
            .map(|line| engine(&line.text).plain())
            .collect()
    }

    /// Each character six GUI pixels wide, as the widget tests fake one.
    fn layout(text: &str, max_width: f32, align: TextAlign, line_spacing: f32) -> TextLines {
        label_layout(
            &fake_font(6.0),
            &crate::text::Text::literal(text),
            max_width,
            align,
            line_spacing,
        )
        .unwrap()
    }

    #[test]
    fn a_label_wraps_at_spaces_and_keeps_its_words() {
        // 48 pixels is eight characters: "a quick" fits, "brown" does not fit after it, and
        // "brown fox" is one character too wide for the second line.
        let lines = layout("a quick brown fox", 48.0, TextAlign::Left, 0.0);
        assert_eq!(plain(&lines), ["a quick", "brown", "fox"]);
        assert_eq!(lines.width, 42.0);
        assert!(
            lines.lines.iter().all(|line| line.width <= 48.0),
            "{:?}",
            lines.lines.iter().map(|l| l.width).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_word_longer_than_the_line_is_broken_at_a_character() {
        let lines = layout("abcdefghij", 24.0, TextAlign::Left, 0.0);
        assert_eq!(plain(&lines), ["abcd", "efgh", "ij"]);
        assert_eq!(plain(&lines).concat(), "abcdefghij");
    }

    #[test]
    fn an_explicit_newline_starts_a_line_and_an_empty_component_has_no_line() {
        assert_eq!(
            plain(&layout("AA\nBB", 100.0, TextAlign::Left, 0.0)),
            ["AA", "BB"]
        );
        let empty = layout("", 100.0, TextAlign::Left, 0.0);
        assert!(empty.lines.is_empty());
        assert_eq!((empty.width, empty.height), (0.0, 0.0));
    }

    #[test]
    fn each_line_is_placed_by_the_alignment() {
        // 24 and 12 pixels wide, in a box of 100: left leaves them at 0, centre at 38 and 44,
        // right at 76 and 88.
        for (align, expected) in [
            (TextAlign::Left, [0.0, 0.0]),
            (TextAlign::Centre, [38.0, 44.0]),
            (TextAlign::Right, [76.0, 88.0]),
        ] {
            let lines = layout("AAAA\nBB", 100.0, align, 0.0);
            let placed: Vec<f32> = lines.lines.iter().map(|line| line.at[0]).collect();
            assert_eq!(placed, expected, "{align:?}");
        }
    }

    #[test]
    fn the_height_is_the_line_count_times_the_line_height_plus_spacing() {
        let lines = layout("A\nB\nC", 100.0, TextAlign::Left, 2.0);
        let down: Vec<f32> = lines.lines.iter().map(|line| line.at[1]).collect();
        assert_eq!(down, [0.0, 11.0, 22.0]);
        assert_eq!(lines.height, 3.0 * 9.0 + 2.0 * 2.0);
        // A viewport sizes itself from the last line rather than the count.
        assert_eq!(lines.height, down.last().unwrap() + 9.0);
    }

    #[test]
    fn styles_survive_wrapping() {
        let mut text = mod_api::Text::literal("AAAA BBBB");
        text.style.bold = Some(true);
        let text: crate::text::Text = text.try_into_engine(&()).unwrap();
        let lines = label_layout(&fake_font(6.0), &text, 30.0, TextAlign::Left, 0.0).unwrap();
        assert_eq!(plain(&lines), ["AAAA", "BBBB"]);
        for line in &lines.lines {
            let line = engine(&line.text);
            assert!(
                line.chars().all(|(c, style)| c == ' ' || style.is_bold()),
                "{:?}",
                line.plain()
            );
        }
    }

    /// A label of 400 characters in a 120-pixel box, with the game's own font: every line fits,
    /// the lines are as many as the text's own width needs, the drawn glyphs cover exactly the
    /// height the layout answered, and 50 such labels lay out inside a frame's 16 ms.
    #[test]
    #[ignore = "daily"]
    fn a_wrapped_label_measures_and_draws_its_own_height() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            let font = &game.gui.atlas.font;
            let text = crate::text::Text::literal("mmmm ".repeat(80));
            let lines = label_layout(font, &text, 120.0, TextAlign::Centre, 0.0).unwrap();
            // The text needs this many 120-pixel lines even with no space wasted.
            let floor = (font.width_text(&text) / 120.0).ceil() as usize;
            assert!(
                lines.lines.len() >= floor,
                "{} lines for a floor of {floor}",
                lines.lines.len()
            );
            assert!(lines.lines.len() >= 20, "{} lines", lines.lines.len());
            for line in &lines.lines {
                assert!(line.width <= 120.0, "{} wide", line.width);
            }
            // What the lines draw: the glyphs' own extent, as the HUD batch records them.
            let mut batch = crate::hud::HudBatch::scratch([1, 1]);
            for line in &lines.lines {
                batch.text_styled(
                    &game.gui.atlas,
                    &engine(&line.text),
                    line.at[0],
                    line.at[1],
                    0xFFFF_FFFF,
                    false,
                );
            }
            let (top, bottom) = batch.vertices.iter().fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(top, bottom), vertex| (top.min(vertex.pos[1]), bottom.max(vertex.pos[1])),
            );
            assert!(
                (bottom - top - lines.height).abs() <= 1.0,
                "drawn {} against a height of {}",
                bottom - top,
                lines.height
            );
            let start = std::time::Instant::now();
            for _ in 0..50 {
                std::hint::black_box(
                    label_layout(font, &text, 120.0, TextAlign::Centre, 0.0).unwrap(),
                );
            }
            // A frame's budget is the release one; an unoptimized test build gets the room its own
            // code needs, and still catches a layout that grows out of hand.
            let budget = if cfg!(debug_assertions) {
                std::time::Duration::from_millis(200)
            } else {
                std::time::Duration::from_millis(16)
            };
            assert!(
                start.elapsed() <= budget,
                "50 labels took {:?} against a budget of {budget:?}",
                start.elapsed()
            );
        });
    }
}
