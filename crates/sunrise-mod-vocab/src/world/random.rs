//! The random a mod draws from: one trait for every stream the game lends, and the owned
//! generator a mod seeds itself.
use sunrise_core::random::{LegacyRandom, RandomSource as EngineRandom};

/// A stream of random draws, with the names and values of vanilla's `RandomSource`.
///
/// Every stream the game lends a mod is a `&mut dyn RandomSource`, and a draw moves the game's own
/// stream in vanilla's order. A mod seeds its own with [`LegacyRandomSource`].
pub trait RandomSource {
    /// `nextInt()`: any `i32`.
    fn next_int(&mut self) -> i32;
    /// `nextInt(bound)`: an `i32` in `0..bound`. Returns 0, without drawing, for a bound below 1,
    /// where vanilla throws.
    fn next_int_bound(&mut self, bound: i32) -> i32;
    /// `nextLong()`.
    fn next_long(&mut self) -> i64;
    /// `nextBoolean()`.
    fn next_boolean(&mut self) -> bool;
    /// `nextFloat()`: in `[0, 1)`.
    fn next_float(&mut self) -> f32;
    /// `nextDouble()`: in `[0, 1)`.
    fn next_double(&mut self) -> f64;
    /// `nextGaussian()`: normally distributed, mean 0 and deviation 1. The stream keeps the
    /// second value of each pair for its next call, as vanilla's does.
    fn next_gaussian(&mut self) -> f64;
    /// `Mth.randomBetweenInclusive(random, min, max)`: an `i32` in `min..=max`, drawn as
    /// `next_int_bound(max - min + 1) + min`. For `max < min` it returns `min` without drawing,
    /// where vanilla throws. The arithmetic wraps rather than panics.
    fn next_int_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        self.next_int_bound(max.wrapping_sub(min).wrapping_add(1))
            .wrapping_add(min)
    }
    /// `nextInt(origin, bound)`: an `i32` in `origin..bound`, drawn as
    /// `next_int_bound(bound - origin) + origin`. For `bound <= origin` it returns `origin`
    /// without drawing, where vanilla throws. The arithmetic wraps rather than panics.
    fn next_int_range(&mut self, origin: i32, bound: i32) -> i32 {
        if origin >= bound {
            return origin;
        }
        self.next_int_bound(bound.wrapping_sub(origin))
            .wrapping_add(origin)
    }
}

/// Vanilla's `LegacyRandomSource`: the generator Java's `Random` is, seeded by the mod.
///
/// It crosses the API by value as the level's random (`level_random`, `level_set_random`) and is
/// what a mod uses for its own seeded draws. Two sources with the same seed draw the same values.
#[derive(Clone, Debug, sunrise_mod_api_macros::Wire)]
#[wire(via = (u64, Option<f64>))]
pub struct LegacyRandomSource(LegacyRandom);

impl LegacyRandomSource {
    /// A source seeded as `new LegacyRandomSource(seed)` is.
    pub fn new(seed: i64) -> Self {
        Self(LegacyRandom::new(seed))
    }
    /// `setSeed(seed)`: restart the stream from `seed`, dropping any cached gaussian.
    pub fn set_seed(&mut self, seed: i64) {
        EngineRandom::set_seed(&mut self.0, seed);
    }
}

/// The whole generator: its 48-bit state and the cached second gaussian, the pair the Wire form
/// carries.
impl From<&LegacyRandomSource> for (u64, Option<f64>) {
    fn from(source: &LegacyRandomSource) -> Self {
        Self::from(&source.0)
    }
}

impl From<(u64, Option<f64>)> for LegacyRandomSource {
    fn from(state: (u64, Option<f64>)) -> Self {
        Self(LegacyRandom::from(state))
    }
}

impl From<LegacyRandom> for LegacyRandomSource {
    fn from(random: LegacyRandom) -> Self {
        Self(random)
    }
}

impl From<LegacyRandomSource> for LegacyRandom {
    fn from(source: LegacyRandomSource) -> Self {
        source.0
    }
}

impl RandomSource for LegacyRandomSource {
    fn next_int(&mut self) -> i32 {
        RandomSource::next_int(&mut self.0)
    }
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        RandomSource::next_int_bound(&mut self.0, bound)
    }
    fn next_long(&mut self) -> i64 {
        RandomSource::next_long(&mut self.0)
    }
    fn next_boolean(&mut self) -> bool {
        RandomSource::next_boolean(&mut self.0)
    }
    fn next_float(&mut self) -> f32 {
        RandomSource::next_float(&mut self.0)
    }
    fn next_double(&mut self) -> f64 {
        RandomSource::next_double(&mut self.0)
    }
    fn next_gaussian(&mut self) -> f64 {
        RandomSource::next_gaussian(&mut self.0)
    }
}

/// The engine's own generator, lent to a mod as it is: each draw is the engine's, so the game's
/// order is kept.
impl RandomSource for LegacyRandom {
    fn next_int(&mut self) -> i32 {
        EngineRandom::next_int(self)
    }
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        EngineRandom::next_int_bound(self, bound).unwrap_or(0)
    }
    fn next_long(&mut self) -> i64 {
        EngineRandom::next_long(self)
    }
    fn next_boolean(&mut self) -> bool {
        EngineRandom::next_boolean(self)
    }
    fn next_float(&mut self) -> f32 {
        EngineRandom::next_float(self)
    }
    fn next_double(&mut self) -> f64 {
        EngineRandom::next_double(self)
    }
    fn next_gaussian(&mut self) -> f64 {
        EngineRandom::next_gaussian(self)
    }
}
