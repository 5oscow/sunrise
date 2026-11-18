//! Core rendering types, GPU layouts, and geometric primitives for Sunrise.

#![no_std]

extern crate alloc;

pub mod frustum;
pub mod terrain;

pub use frustum::{Cull, Frusta, Frustum, JoinedFrusta, MAX_WORLD_VIEWS};
pub use terrain::{
    CHUNK_VISIBILITY_THRESHOLD, SectionKey, UV_STEPS, chunk_visibility, fade_millis,
    section_instance,
};
