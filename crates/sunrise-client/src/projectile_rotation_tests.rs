//! Independent 26.3 Projectile.shootFromRotation sequence fixtures, and
//! how a wind charge's tick turns it (`AbstractHurtingProjectile.tick`),
//! from a saved rotation too.
use super::{Projectile, ProjectileKind, ProjectileWorld};
use crate::entity_nbt::fireball_from_nbt;
use crate::mobs::Who;
use crate::save::{doubles, floats};
use sunrise_core::{
    geometry::{Aabb, Vec3},
    nbt::Tag,
    random::{LegacyRandom, RandomSource},
};

/// Open air: nothing to hit, no water.
struct Air;
impl ProjectileWorld for Air {
    fn clip(&self, _: Vec3, _: Vec3) -> Option<(Vec3, [i32; 3], [i32; 3])> {
        None
    }
    fn in_water(&self, _: Aabb) -> bool {
        false
    }
    fn targets(&self) -> &[(Who, Aabb)] {
        &[]
    }
}

#[test]
fn rotation_shot_consumes_six_doubles_in_axis_order_and_inherits_motion() {
    // java.util.Random(1234).nextDouble():
    // .6465821602909256, .9513577109193919, .8575884598068334,
    // .45823330506267057, .3359524025416939, .20387478195313158.
    // Vanilla Projectile.shoot: normalize [0,0,1], add x/y/z triangles
    // scaled by .0172275, multiply by 1.5, then add known movement.
    for grounded in [false, true] {
        let mut shot = Projectile::new(ProjectileKind::Snowball, Vec3::ZERO, None)
            .with_entity_random(LegacyRandom::new(1234));
        shot.shoot_from_rotation(0.0, 0.0, 0.0, 1.5, 1.0, Vec3::new(0.2, 0.3, -0.4), grounded);
        let expected_y = if grounded {
            0.0103198363925326
        } else {
            0.3103198363925326
        };
        for (actual, expected) in [
            (shot.velocity.x, 0.19212421880232217),
            (shot.velocity.y, expected_y),
            (shot.velocity.z, 1.1034130508130342),
        ] {
            assert!((actual - expected).abs() < 1e-14, "{actual} != {expected}");
        }
        assert_eq!(shot.entity_random().next_double(), 0.34690742873967684);
        assert!(shot.needs_sync);
    }
}

#[test]
fn rotation_shot_applies_vanilla_near_zero_normalization_without_skipping_draws() {
    let mut shot = Projectile::new(ProjectileKind::Snowball, Vec3::ZERO, None)
        .with_entity_random(LegacyRandom::new(1234));
    // At pitch 90 with a -90 vertical offset all three direction components
    // are below Vec3.normalize's widened 1.0E-5F threshold. Spread is zero,
    // but Projectile.getMovementToShoot still takes all six double draws.
    shot.shoot_from_rotation(90.0, 0.0, -90.0, 1.5, 0.0, Vec3::new(0.2, 0.3, -0.4), false);
    assert_eq!(shot.velocity, Vec3::new(0.2, 0.3, -0.4));
    assert_eq!(shot.entity_random().next_double(), 0.34690742873967684);
    assert!(shot.needs_sync);
}

/// `AbstractWindCharge.tick` runs `AbstractHurtingProjectile.tick`, which
/// calls `ProjectileUtil.rotateTowardsMovement(this, 0.2F)` before it moves:
/// a fifth of the way from the tick's old rotation towards the motion.
/// Vanilla 26.3, `/summon wind_charge ~ ~ ~ {Motion:[0.6d,0.0d,0.0d]}` then
/// `/data get entity .. Rotation` after each `tick step 1` (#765).
#[test]
fn a_wind_charge_turns_a_fifth_of_the_way_to_its_motion_each_tick() {
    for kind in [ProjectileKind::WindCharge, ProjectileKind::BreezeWindCharge] {
        let mut p = Projectile::new(kind.clone(), Vec3::new(0.5, 100.0, 0.5), None);
        p.velocity = Vec3::new(0.6, 0.0, 0.0);
        let mut seen = Vec::new();
        for _ in 0..3 {
            p.tick(&Air);
            seen.push([p.yaw, p.pitch]);
        }
        assert_eq!(
            seen,
            [
                [18.0_f32, -1.525_878_9e-6],
                [32.4, -2.746_582_1e-6],
                [43.920_002, -3.723_144_8e-6],
            ],
            "{kind:?}"
        );
        // The charge's motion is untouched by the turn (inertia 1.0, no power).
        assert_eq!(p.velocity, Vec3::new(0.6, 0.0, 0.0), "{kind:?}");
    }
}

/// `rotateTowardsMovement` returns at once for a zero motion: a still wind
/// charge keeps the rotation it was summoned with. Vanilla 26.3,
/// `/summon wind_charge .. {Rotation:[-170.5f,-90.0f]}` reads back
/// `[-170.5f, -90.0f]` after its ticks; it never snaps to `[0f, 0f]`.
#[test]
fn a_still_wind_charge_keeps_its_rotation() {
    for kind in [ProjectileKind::WindCharge, ProjectileKind::BreezeWindCharge] {
        let mut p = Projectile::new(kind.clone(), Vec3::new(0.5, 100.0, 0.5), None);
        p.yaw = -170.5;
        p.pitch = -90.0;
        for _ in 0..3 {
            p.tick(&Air);
        }
        assert_eq!([p.yaw, p.pitch], [-170.5, -90.0], "{kind:?}");
    }
}

/// A moving wind charge restored with a huge finite rotation (`Rotation`
/// is loaded as saved; `Entity.setYRot` keeps any finite yaw) must tick
/// and turn. Vanilla's `rotateTowardsMovement` loops would never end here:
/// `1.0E30F - 360.0F == 1.0E30F`. Sunrise first takes the angle within one
/// turn with `%`: `1.0E30F % 360.0F` is 120.0 and `Float.MAX_VALUE %
/// 360.0F` is 0.0 (Java `frem`). From there the 26.3 loops and
/// `Mth.lerp(0.2F, ..)` give the rotations below (computed with the jar's
/// `Mth`), and the same three ticks as from a sane start. Fireballs,
/// dragon fireballs and wither skulls load through the same
/// `fireball_from_nbt` and turn through the same function.
#[test]
fn a_restored_wind_charge_with_a_huge_rotation_turns_back_into_range() {
    for (id, rotation) in [
        ("minecraft:wind_charge", [1.0e30_f32, f32::MAX]),
        ("minecraft:breeze_wind_charge", [1.0e30, f32::MAX]),
        ("minecraft:small_fireball", [1.0e30, f32::MAX]),
    ] {
        let tag = Tag::compound([
            ("id", Tag::String(id.into())),
            ("Pos", doubles(&[0.5, 100.0, 0.5])),
            ("Motion", doubles(&[0.6, 0.0, 0.0])),
            ("Rotation", floats(&rotation)),
            ("acceleration_power", Tag::Double(0.0f64.to_bits())),
        ]);
        let mut p = fireball_from_nbt(&tag, None).unwrap();
        // Vanilla's `setXRot` would load the pitch as `clamp(MAX % 360, -90,
        // 90)` = 0; Sunrise's loader keeps it unclamped (as on main), and
        // `shift_by_turns` reduces it to that same 0 on the first turn.
        assert_eq!(p.yaw, 1.0e30, "{id}");
        let mut seen = Vec::new();
        for _ in 0..3 {
            p.tick(&Air);
            seen.push([p.yaw, p.pitch]);
        }
        assert_eq!(
            seen,
            [
                [114.0_f32, -1.525_878_9e-6],
                [109.2, -2.746_582_1e-6],
                [105.36, -3.723_144_8e-6],
            ],
            "{id}"
        );
    }
}

/// A non-finite saved angle, which `Entity.setYRot` / `setXRot` refuse on
/// load (the rotation stays 0), turns as from `[0f, 0f]`: the #765 values.
#[test]
fn a_restored_wind_charge_with_an_infinite_rotation_turns_as_from_zero() {
    let tag = Tag::compound([
        ("id", Tag::String("minecraft:wind_charge".into())),
        ("Pos", doubles(&[0.5, 100.0, 0.5])),
        ("Motion", doubles(&[0.6, 0.0, 0.0])),
        ("Rotation", floats(&[f32::INFINITY, f32::NEG_INFINITY])),
    ]);
    let mut p = fireball_from_nbt(&tag, None).unwrap();
    let mut seen = Vec::new();
    for _ in 0..3 {
        p.tick(&Air);
        seen.push([p.yaw, p.pitch]);
    }
    assert_eq!(
        seen,
        [
            [18.0_f32, -1.525_878_9e-6],
            [32.4, -2.746_582_1e-6],
            [43.920_002, -3.723_144_8e-6],
        ]
    );
}
