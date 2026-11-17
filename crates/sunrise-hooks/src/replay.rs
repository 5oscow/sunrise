//! Session recording and playback. The engine feeds and consumes play packets; a provider
//! above the engine owns the archive format, the recorder and the playback driver.

use crate::{TooLate, registry::Registry};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    time::Duration,
};
use sunrise_protocol::packets::play;

/// A raw timestamped clientbound network packet from a replay stream.
#[derive(Clone, Debug, PartialEq)]
pub struct TimestampedPacket {
    /// Milliseconds since recording start.
    pub timestamp_ms: u32,
    /// Protocol packet ID.
    pub id: i32,
    /// Unframed packet payload bytes (excluding the VarInt packet ID).
    pub payload: Vec<u8>,
}

impl TimestampedPacket {
    pub fn new(timestamp_ms: u32, id: i32, payload: Vec<u8>) -> Self {
        Self {
            timestamp_ms,
            id,
            payload,
        }
    }
}

/// A replay being played: its timeline, its speed and the packets that come due.
pub trait PacketSource {
    /// Current timeline position in milliseconds.
    fn position_ms(&self) -> u32;
    /// Total recording duration in milliseconds.
    fn duration_ms(&self) -> u32;
    /// Total number of indexed packets in the recording stream.
    fn packet_count(&self) -> usize;
    /// Current playback progress as a fraction from 0.0 to 1.0.
    fn progress(&self) -> f32;
    /// Current playback speed multiplier (e.g. 0.25, 1.0, 2.0).
    fn speed(&self) -> f64;
    /// Set the playback speed multiplier.
    fn set_speed(&mut self, speed: f64);
    /// Whether playback is paused.
    fn is_paused(&self) -> bool;
    /// Set the paused state.
    fn set_paused(&mut self, paused: bool);
    /// Toggle the paused state.
    fn toggle_pause(&mut self);
    /// Set whether the replay loops when it reaches the end.
    fn set_looping(&mut self, looping: bool);
    /// Whether playback reached the end of the recording.
    fn is_finished(&self) -> bool;
    /// Advance virtual time by `delta` and collect all packets scheduled up to the new time.
    fn advance(&mut self, delta: Duration) -> io::Result<Vec<TimestampedPacket>>;
    /// Seek directly to a timestamp in milliseconds.
    fn seek(&mut self, target_ms: u32) -> io::Result<()>;
    /// Seek from the beginning and return all packets needed to rebuild the state at `target_ms`.
    fn seek_and_collect(&mut self, target_ms: u32) -> io::Result<Vec<TimestampedPacket>>;
    /// Move forward from the current position and return the packets crossed.
    fn seek_forward(&mut self, target_ms: u32) -> io::Result<Vec<TimestampedPacket>>;
    /// Rewind playback to the beginning.
    fn restart(&mut self) -> io::Result<()>;
    /// Archive annotations, for sources that support them.
    fn annotations(&self) -> Option<&dyn Annotations> {
        None
    }
    /// Editable archive annotations, for sources that support them.
    fn annotations_mut(&mut self) -> Option<&mut dyn Annotations> {
        None
    }
}

/// A ReplayMod-format bookmark stored with a replay, without player identity data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub time_ms: u32,
    pub label: String,
}

/// Optional archive data separate from the game packets.
pub trait Annotations {
    fn markers(&self) -> &[Marker];
    /// Save a bookmark at the current playhead.
    fn add_marker(&mut self, label: String) -> io::Result<Marker>;
    /// Read a named archive extension without interpreting its contents.
    fn read_extension(&self, name: &str) -> io::Result<Option<Vec<u8>>>;
    /// Atomically replace the named extension, or remove it with `None`.
    fn replace_extension(&mut self, name: &str, bytes: Option<&[u8]>) -> io::Result<()>;
}

/// A recording of the session's packets. The network thread supplies received packets; the
/// game thread supplies the client's own movement, pose and swings and the initial snapshot.
pub trait PacketRecorder: Send {
    /// Whether it still takes packets (false once stopped or failed).
    fn is_active(&self) -> bool;
    /// A play packet the network thread received, decoded, with its id and payload as they came.
    /// The recorder decides what it keeps.
    fn received(&self, packet: &play::Clientbound, id: i32, payload: &[u8]);
    /// A packet the client made for the recording, encoded, stamped now.
    fn record_packet(&self, id: i32, payload: &[u8]);
    /// An encoded packet in the initial snapshot, before any live packet. A timed recorder
    /// overrides this to stamp it at zero; an untimed tap reads it like any other local packet.
    fn record_initial_packet(&self, id: i32, payload: &[u8]) {
        self.record_packet(id, payload);
    }
    fn status(&self) -> RecorderStatus;
    /// Finishes the recording; the file it wrote.
    fn stop(self: Box<Self>) -> io::Result<PathBuf>;
}

impl<T: PacketRecorder + 'static> From<T> for Box<dyn PacketRecorder> {
    fn from(recorder: T) -> Self {
        Box::new(recorder)
    }
}

/// Where a recording stands.
#[derive(Clone, Debug, PartialEq)]
pub struct RecorderStatus {
    pub active: bool,
    pub path: PathBuf,
    pub elapsed: Duration,
    pub packet_count: u64,
    pub bytes_written: u64,
    pub error: Option<String>,
}

/// Opens packet recordings without the engine knowing their archival format.
pub trait Factory: Sync {
    fn start_recording(
        &self,
        path: PathBuf,
        server_name: String,
        singleplayer: bool,
    ) -> io::Result<Box<dyn PacketRecorder>>;
    fn open_replay(&self, path: &Path) -> io::Result<Box<dyn PacketSource>>;
}

static FACTORIES: Registry<dyn Factory> = Registry::new();

/// Register a replay provider before the game starts. The first installed provider is used.
pub fn register(factory: &'static dyn Factory) -> Result<(), TooLate> {
    FACTORIES.register(factory)
}

pub mod engine {
    use super::*;

    /// The installed provider, if any. With no provider, offer no replay commands or screen.
    pub fn factory() -> Option<&'static dyn Factory> {
        FACTORIES.all().first().copied()
    }
}
