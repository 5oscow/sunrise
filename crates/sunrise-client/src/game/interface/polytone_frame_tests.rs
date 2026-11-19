//! A frame whose only particles are a pack's GPU particle records (wiki "GPU Particles"): the
//! records aren't in the native particle count, and the frame still builds their quads when no
//! renderer draws the type's shader.
use crate::game::native_entity_refs::tests::with_game;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use sunrise_assets::resource_source::{Resource, ResourceSource};
use sunrise_client::hud::Rect;
use sunrise_client::particles::polytone::Registry;
use sunrise_client::particles::{ParticleEngine, ParticleOptions};

/// A pack with one GPU particle type, `test:rain`, drawn with a vanilla particle sprite and
/// no shader of its own.
fn rain_pack() -> ResourceSource {
    let rain = serde_json::json!({
        "type": "gpu", "texture": "minecraft:textures/particle/generic_0.png",
        "render_type": "translucent", "initializer": {"lifetime": "40", "size": "0.5"}
    });
    ResourceSource::indexed(
        [(
            "assets/test/polytone/custom_particles/rain.json".to_owned(),
            Resource::bytes(rain.to_string().into_bytes()),
        )]
        .into_iter()
        .collect(),
    )
}

#[test]
#[ignore = "daily"]
fn a_frame_of_gpu_records_only_draws_their_quads() {
    with_game(|game| {
        let registry = Registry::read(&rain_pack(), Default::default(), game.pack.assets.registry);
        // The rain's sprite in the particles' atlas, as a resource load puts it there.
        let rect = Rect {
            u0: 0.0,
            v0: 0.0,
            u1: 1.0,
            v1: 1.0,
            w: 16,
            h: 16,
        };
        game.particles = ParticleEngine::new(
            &rain_pack(),
            &HashMap::from([("particle/generic_0".to_owned(), rect)]),
            &HashSet::new(),
        );
        game.particles.set_polytone(Some(Arc::new(registry)));
        let at = [0.5, 80.0, 0.5];
        game.particles.camera = at;
        game.particles
            .add(&ParticleOptions::named("test:rain"), at, [0.0; 3]);
        assert_eq!(game.client_particle_count(), 0, "not in the native count");
        assert_eq!(game.particles.polytone_gpu_records(), 1);
        let camera = glam::DVec3::new(at[0], at[1] + 2.0, at[2]);
        game.polytone_gpu_decide(camera);
        let mut out = Vec::new();
        game.client_particle_geometry(camera, 0.0, 70.0, &mut out);
        assert_eq!(out.len(), 4, "the record's quad");
    });
}
