//! A feature placed by a native mod (`LevelRef::place_feature`), as `/place feature`
//! places one: `PlaceCommand.placeFeature` without the command's feedback.
use super::Game;
use mod_api::{LevelReadError, LevelRef, PlaceFeatureRefusal};

impl Game<'_> {
    /// `Feature.place` of `feature` at `pos` in the level in place: the feature's own reported
    /// answer, so `Ok(false)` does not promise that no block was written.
    pub(super) fn level_place_feature(
        &mut self,
        level: LevelRef,
        feature: &str,
        pos: [i32; 3],
    ) -> Result<bool, PlaceFeatureRefusal> {
        match self.native_level_world_result(level) {
            Err(LevelReadError::Unavailable) => return Err(PlaceFeatureRefusal::Client),
            Err(_) => return Err(PlaceFeatureRefusal::Stale),
            Ok(_) if level.dimension() != self.dim.key() => {
                return Err(PlaceFeatureRefusal::NotInPlace);
            }
            Ok(_) => {}
        }
        if !self.is_logical_server() {
            return Err(PlaceFeatureRefusal::Client);
        }
        // `PlaceCommand.checkLoaded` over the feature's chunk and its ring, then the level's own
        // write gate at `pos` (height, a carrier fence).
        let (cx, cz) = (pos[0] >> 4, pos[2] >> 4);
        let ring_loaded = (cx - 1..=cx + 1)
            .all(|x| (cz - 1..=cz + 1).all(|z| self.world.chunks.contains_key(&(x, z))));
        if !ring_loaded || !self.native_level_write_allowed(level, pos) {
            return Err(PlaceFeatureRefusal::NotLoaded);
        }
        // The live features the world installed as it opened, as `/place feature` reads them.
        let known = feature.contains(':')
            && self
                .res
                .behaviours()
                .live_features()
                .is_some_and(|features| features.configured(feature).is_some());
        if !known {
            return Err(PlaceFeatureRefusal::UnknownFeature);
        }
        let at = sunrise_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
        let placed = self
            .with_level(|b, level, ticks| b.place_feature(level, ticks, feature, at))
            .ok_or(PlaceFeatureRefusal::UnknownFeature)?;
        let _ = self.process_level_events();
        Ok(placed)
    }
}
