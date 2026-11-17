//! The ReplayMod-compatible `.mcpr` format, session recorder and packet playback.
#![forbid(unsafe_code)]

pub mod mcpr;
pub mod recorder;
pub mod replay;

use std::{
    io,
    path::{Path, PathBuf},
};
use sunrise_hooks::replay::{self as hooks, Factory, PacketRecorder, PacketSource};

struct Mcpr;
static MCPR: Mcpr = Mcpr;

/// Install the `.mcpr` provider before starting the game.
pub fn register() -> Result<(), sunrise_hooks::TooLate> {
    hooks::register(&MCPR)
}

impl Factory for Mcpr {
    fn start_recording(
        &self,
        path: PathBuf,
        server_name: String,
        singleplayer: bool,
    ) -> io::Result<Box<dyn PacketRecorder>> {
        let metadata = mcpr::Metadata::new(
            server_name,
            singleplayer,
            sunrise_core::release::VERSION.into(),
        );
        Ok(Box::new(recorder::LiveRecorder::start(path, metadata)?))
    }

    fn open_replay(&self, path: &Path) -> io::Result<Box<dyn PacketSource>> {
        Ok(Box::new(replay::ReplayDriver::open(path)?))
    }
}
