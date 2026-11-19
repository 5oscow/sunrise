//! The volume of the audio mods make themselves (`ClientHost::open_audio_stream`), which a crate
//! above the engine sets: 1 plays it as the mods ask, 0 plays none of it. The engine owns the
//! value and reads it where it mixes; a crate pushes it when its setting changes, never per frame.
//! Nothing set, the mods' audio plays as asked.
use std::sync::atomic::{AtomicU32, Ordering};

/// The gain's bits: 1.0 until a crate sets it.
static MOD_STREAM_GAIN: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// Sets the gain on every mod audio stream, from 0 to 1 (anything else is clamped, NaN is 0). At 0
/// the engine mixes none of it: the streams stay open and their audio is still taken in at its own
/// pace, so a mod needs no special case and no delay builds up.
pub fn set_mod_stream_gain(gain: f32) {
    let gain = if gain.is_nan() {
        0.0
    } else {
        gain.clamp(0.0, 1.0)
    };
    MOD_STREAM_GAIN.store(gain.to_bits(), Ordering::Relaxed);
}

/// The engine's side.
pub mod engine {
    use super::*;

    /// [`set_mod_stream_gain`]'s value.
    pub fn mod_stream_gain() -> f32 {
        f32::from_bits(MOD_STREAM_GAIN.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gain_is_one_until_set_and_stays_within_zero_and_one() {
        assert_eq!(engine::mod_stream_gain(), 1.0);
        set_mod_stream_gain(0.25);
        assert_eq!(engine::mod_stream_gain(), 0.25);
        set_mod_stream_gain(7.0);
        assert_eq!(engine::mod_stream_gain(), 1.0);
        set_mod_stream_gain(f32::NAN);
        assert_eq!(engine::mod_stream_gain(), 0.0);
        set_mod_stream_gain(1.0);
    }
}
