//! Core GPU occlusion culling structures, bounding boxes, and records.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// In `Record::a[3]`: the CPU draws the section's solid quads while it fades,
/// and the GPU culling pass leaves it out.
pub const FADING: u32 = 1;

/// `rounded_box` for bounds past what a box's codes hold, or not finite: no
/// frustum plane rejects it, so it is drawn wherever its column is within the
/// render distance.
pub const UNBOUNDED: u32 = 1 << 31;

/// What a box's ends, -8 to 23 blocks from its section's origin, add to make
/// their 5-bit codes.
pub const CODE_OFFSET: i32 = 8;

/// A record in GPU culling's table: its bounding box, arena offsets, quad counts, and facing keys.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Record {
    /// Section x, y (from the level's bottom), z and arena page; -1: a free id.
    pub key: [i32; 4],
    /// The first vertex of its solid quads, its solid quads count, its box (`rounded_box`), then flags (`FADING`).
    pub a: [u32; 4],
    /// Where its first six buckets end.
    pub ends: [u32; 6],
    /// What its buckets compare the eye's coordinates to.
    pub keys: [u32; 6],
}

impl Record {
    pub const BYTES: u64 = core::mem::size_of::<Self>() as u64;

    pub const FREE: Self = Self {
        key: [0, 0, 0, -1],
        a: [0; 4],
        ends: [0; 6],
        keys: [0; 6],
    };

    pub fn new(key: (i32, i32, i32), page: u32, start: u64, quads: u32, boxed: u32) -> Self {
        Self {
            key: [key.0, key.1, key.2, page as i32],
            a: [start as u32, quads, boxed, 0],
            ends: [0; 6],
            keys: [0; 6],
        }
    }

    pub fn page(&self) -> Option<u32> {
        (self.key[3] >= 0).then_some(self.key[3] as u32)
    }

    pub fn is_fading(&self) -> bool {
        self.a[3] & FADING != 0
    }
}

/// A record's box: its section's quad bounds rounded out to whole blocks.
/// Packed as 5-bit codes: the low ends along x, y and z in bits 0 to 14, the high ends in bits 15 to 29.
pub fn rounded_box(bounds: Option<[Vec3; 2]>) -> u32 {
    let [min, max] = bounds.unwrap_or([Vec3::ZERO, Vec3::splat(16.0)]);
    let mut packed = 0;
    for axis in 0..3 {
        if !(min[axis].is_finite() && max[axis].is_finite()) {
            return UNBOUNDED;
        }
        let low = (min[axis].floor() as i32).min(16);
        let high = (max[axis].ceil() as i32).max(0);
        if low < -CODE_OFFSET || high > 31 - CODE_OFFSET {
            return UNBOUNDED;
        }
        packed |= (((low + CODE_OFFSET) as u32) << (5 * axis))
            | (((high + CODE_OFFSET) as u32) << (15 + 5 * axis));
    }
    packed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_record_has_no_page() {
        assert_eq!(Record::FREE.page(), None);
        assert!(!Record::FREE.is_fading());
    }

    #[test]
    fn rounded_box_default_cell() {
        let default_box = rounded_box(None);
        assert_ne!(default_box, UNBOUNDED);
    }

    #[test]
    fn rounded_box_infinite_is_unbounded() {
        assert_eq!(rounded_box(Some([Vec3::NAN, Vec3::splat(16.0)])), UNBOUNDED);
    }
}
