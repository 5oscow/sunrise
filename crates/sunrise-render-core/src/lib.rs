//! Core rendering types, GPU layouts, and geometric primitives for Sunrise.

#![no_std]

extern crate alloc;

pub mod frustum;

pub use frustum::{Cull, Frusta, Frustum, JoinedFrusta, MAX_WORLD_VIEWS};
