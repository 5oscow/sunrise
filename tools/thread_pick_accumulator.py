"""One-off mechanical threading of the shared entity-pick accumulator."""
from pathlib import Path
ROOT = Path('crates/sunrise-client/src')

def transform(path, name, args, ret, edits):
    p = ROOT / path
    text = p.read_text()
    start = text.index('    ', text.index('fn '+name)-30) if False else text.rfind('\n', 0, text.index('fn '+name))+1
    body = text.index('{', text.index('fn '+name))
    depth = 1
    end = body+1
    while depth:
        depth += (text[end]=='{') - (text[end]=='}')
        end += 1
    original = text[start:end]
    signature = original[:original.index('{')]
    wrapper = signature + '{\n        let mut pick = crate::game::pick_accumulator::PickAccumulator::new(self.entity_reach);\n        self.'+name+'_with_pick('+args+'&mut pick)\n    }\n'
    internal = original.replace('fn '+name+'(', 'fn '+name+'_with_pick(',1)
    # Add accumulator before the signature closes.
    pos = internal.index(') ->')
    internal = internal[:pos].rstrip().rstrip(',')+', pick: &mut crate::game::pick_accumulator::PickAccumulator'+internal[pos:]
    for old,new in edits:
        assert old in internal, (path,name,old)
        internal=internal.replace(old,new)
    p.write_text(text[:start]+wrapper+internal+text[end:])

for path,name,var in [('game/entities/boats/mod.rs','boat_hit','b'),('game/entities/minecarts.rs','cart_hit','c'),('game/entities/armor_stands.rs','armor_stand_hit','a'),('game/entities/frames.rs','frame_hit','f'),('game/entities/paintings.rs','painting_hit','p'),('game/mobs/dragon.rs','dragon_hit',None)]:
    edits=[('best.is_none_or(|b| t0 < b.1)',f'pick.consider(self, {var+".id" if var else "id"}, t0)')]
    if name in ['boat_hit','cart_hit']:
        edits.append((f'            if Some({var}.id) == self.riding {{\n                continue;\n            }}\n',''))
    if name=='armor_stand_hit':
        edits.append(('a.marker || self.remote_camera_id() == Some(a.id)','a.marker'))
    transform(path,name,'','',edits)
transform('game/entities/cushions.rs','cushion_hit','','', [('best.is_none_or(|b| d < b.1)','pick.consider(self, c.id, d)')])
transform('game/items/deflect.rs','redirectable_hit','','', [('.map(|t| (i, t))','.filter(|&t| pick.consider(self, p.id, t)).map(|t| (i, t))'),('.min_by(|a, b| a.1.total_cmp(&b.1))','.last()')])
transform('game.rs','mob_hit_within','reach, ','', [('m.removed || Some(m.id) == self.riding || Some(m.id) == self.remote_camera_id()','m.removed'),('best.is_none_or(|b| t < b.1)','pick.consider(self, m.id, t)')])
transform('game/network/remote_vehicles.rs','remote_knot_hit','','', [('.map(|d| (id as i32, d, [p.x, p.y, p.z]))','.filter(|&d| pick.consider(self, id, d)).map(|d| (id as i32, d, [p.x, p.y, p.z]))'),('.min_by(|a, b| a.1.total_cmp(&b.1))','.last()')])
transform('game/network/remote_vehicles.rs','remote_object_hit','attacking, ','', [(f'self.{n}()',f'self.{n}_with_pick(pick)') for n in ['painting_hit','frame_hit','armor_stand_hit','cushion_hit','boat_hit','cart_hit','remote_knot_hit']]+[('ray_box(eye, dir, reach, i.bounds())','ray_box(eye, dir, reach, i.bounds()).filter(|&d| pick.consider(self, id as u32, d))'),('ray_box(eye, dir, reach, bounds)','ray_box(eye, dir, reach, bounds).filter(|&d| pick.consider(self, c.id, d))'),('self.remote_dragon_part_hit(eye, dir, reach)','self.remote_dragon_part_hit_with_pick(eye, dir, reach, pick)'),('hits.into_iter().min_by(|a, b| a.1.total_cmp(&b.1))','hits.into_iter().find(|hit| pick.hovered.is_some_and(|hovered| hovered.id == hit.0 as u32))')])
transform('game/mobs/dragon.rs','remote_dragon_part_hit','eye, dir, reach, ','', [('.map(|t| ((d.id + 1 + i as u32) as i32, t, [p.x, p.y, p.z]))','.filter(|&t| pick.consider(self, d.id + 1 + i as u32, t)).map(|t| ((d.id + 1 + i as u32) as i32, t, [p.x, p.y, p.z]))'),('.min_by(|a, b| a.1.total_cmp(&b.1))','.last()')])
