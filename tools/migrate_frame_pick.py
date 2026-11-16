"""Replace action-side ray refreshes with the shared client frame decision."""
from pathlib import Path

root = Path('crates/sunrise-client/src')

def replace(path, old, new):
    p = root / path
    text = p.read_text()
    assert text.count(old) == 1, (path, old)
    p.write_text(text.replace(old, new))

families = [
 ('game/entities/frames.rs', 'frame_hit', 'item_frames', 'f.id', False),
 ('game/entities/paintings.rs', 'painting_hit', 'paintings', 'f.id', False),
 ('game/entities/armor_stands.rs', 'armor_stand_hit', 'armor_stands', 'f.id', True),
 ('game/entities/cushions.rs', 'cushion_hit', 'cushions', 'f.id', True),
 ('game/entities/boats/mod.rs', 'boat_hit', 'boats', 'f.id', False),
 ('game/entities/minecarts.rs', 'cart_hit', 'carts', 'f.id', False),
 ('game/items/deflect.rs', 'redirectable_hit', 'creatures.projectiles', 'f.id', False),
]
for path, name, collection, identity, point in families:
    p = root / path
    text = p.read_text()
    start = text.index('fn ' + name + '(')
    body = text.index('{', start) + 1
    result = '(index, distance, point)' if point else '(index, distance)'
    point_binding = 'point' if point else '_'
    text = text[:body] + f'''
        if let Some(hit) = self.cached_entity_pick() {{
            let (entity, distance, {point_binding}) = hit?;
            let index = self.{collection}.iter().position(|f| {identity} == entity.id())?;
            return Some({result});
        }}''' + text[body:]
    p.write_text(text)

replace('game.rs', 'client_targeting: std::cell::Cell<mod_api::ClientTargeting>,', 'frame_pick: client_frame_pick::FramePick,')
replace('game.rs', 'client_targeting: Default::default(),', 'frame_pick: Default::default(),')
replace('game.rs', '''        // Minecraft.pick: from the camera entity's eyes along its view''', '''        if let Some(ray) = self.frame_pick.ray {
            return Some(ray);
        }
        // Minecraft.pick: from the camera entity's eyes along its view''')
replace('game.rs', '''        if let Some(hit) = self.tick_hit.get() {
            return hit;
        }''', '''        if self.uses_frame_pick() {
            return match self.crosshair_target() {
                Some(mod_api::CrosshairTarget::Block { pos, hit, .. }) => Some(Hit {
                    pos: [pos.x, pos.y, pos.z], face: hit.face, point: hit.point, inside: hit.inside,
                }),
                _ => None,
            };
        }
        if let Some(hit) = self.tick_hit.get() {
            return hit;
        }''')
replace('game.rs', 'self.clip_targeting_ray(origin, dir, self.block_reach.max(self.entity_reach))', 'self.clip_ray(origin, dir, self.block_reach.max(self.entity_reach))')
replace('game.rs', '''    fn mob_hit(&self) -> Option<(usize, f64, [f64; 3])> {
''', '''    fn mob_hit(&self) -> Option<(usize, f64, [f64; 3])> {
        if let Some(hit) = self.cached_entity_pick() {
            let (entity, distance, point) = hit?;
            let index = self.mobs.iter().position(|m| m.id == entity.id())?;
            let feet = self.mobs[index].position;
            return Some((index, distance, [point[0] - feet.x, point[1] - feet.y, point[2] - feet.z]));
        }
''')
replace('game.rs', '        let alpha = self.frame_world_partial_tick();', '''        let alpha = self.frame_world_partial_tick();
        self.refresh_frame_pick(alpha as f32);''')
replace('game/interface/perspective.rs', '    fn player_camera_entity(&mut self)', '    pub(super) fn player_camera_entity(&mut self)')
replace('game/entities/frames.rs', '''        if self.native_window() && self.targeting().hit_through_walls {
            return None;
        }''', '''        if self.uses_frame_pick() {
            return match self.crosshair_target() {
                Some(mod_api::CrosshairTarget::Block { hit, .. }) => Some(self.frame_pick_distance(hit.point)),
                _ => None,
            };
        }''')
replace('game/items/hand.rs', '''        if let Some(entity) = self.targeting().entity {
            return self.targeting_entity_hit(entity).is_some();
        }''', '''        if self.uses_frame_pick() {
            return matches!(self.crosshair_target(), Some(mod_api::CrosshairTarget::Entity { .. }));
        }''')
replace('game/items/hand.rs', '''    pub(super) fn pick_tick_targets(&mut self) {
''', '''    pub(super) fn pick_tick_targets(&mut self) {
        if self.native_window() {
            self.select_queued_hotbar();
            return;
        }
''')
replace('game/items/interactions.rs', '''    pub(super) fn interaction_hit(&self) -> Option<(i32, f64)> {
''', '''    pub(super) fn interaction_hit(&self) -> Option<(i32, f64)> {
        if let Some(hit) = self.cached_entity_pick() {
            let (entity, distance, _) = hit?;
            return self.interactions.contains_key(&(entity.id() as i32)).then_some((entity.id() as i32, distance));
        }
''')
replace('game/mobs/dragon.rs', '''    pub(super) fn dragon_hit(&self) -> Option<(u32, f64)> {
''', '''    pub(super) fn dragon_hit(&self) -> Option<(u32, f64)> {
        if let Some(hit) = self.cached_entity_pick() {
            let (entity, distance, _) = hit?;
            return self.special_targets().any(|(id, _)| id == entity.id()).then_some((entity.id(), distance));
        }
''')
