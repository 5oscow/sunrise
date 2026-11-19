//! Client mods' audio streams (`open_audio_stream`): the game opens them on the mixer's own pool and
//! places the entity-following ones each frame.
use super::Game;
use mod_api::{AudioRefusal, AudioStreamRef, StreamSpec};

impl Game<'_> {
    /// Opens a stream for the calling mod, on the mixer's pool for mods' audio. Err on a server,
    /// without a sound output, for a spec out of range and past the 32 streams a mod or the 64
    /// all mods may hold.
    pub(in crate::game) fn open_audio_stream(
        &self,
        spec: &StreamSpec,
    ) -> Result<AudioStreamRef, AudioRefusal> {
        if !self.native_window() {
            return Err(AudioRefusal::OnServer);
        }
        let owner = mod_api::content::crash::current_native_mod().unwrap_or_default();
        self.audio.open_stream(owner, spec).map(AudioStreamRef::new)
    }
    /// Places every entity-following stream at its entity, or leaves it unplaced where the client
    /// tracks no such entity. Once a frame, beside the listener's own update.
    pub(in crate::game) fn follow_audio_stream_entities(&self) {
        self.audio
            .follow_stream_entities(|entity| self.entity_position(entity));
    }
}
