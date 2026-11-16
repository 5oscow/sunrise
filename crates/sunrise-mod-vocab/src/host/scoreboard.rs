//! Host method declarations; access defaults and game forwarders are generated.
#![allow(unused_variables)]
use crate::*;
/// The server host's methods of the `scoreboard` area, implemented by the game and reached through [`ServerHost`].
///
/// Every write is the one `/scoreboard` makes on the server's board, so clients, saves and selectors
/// cannot tell a mod's write from the command. Inside `Host::transaction` the writes apply at once
/// and an abort puts the board back as it was when the transaction first wrote it.
#[sunrise_mod_api_macros::host_area]
pub trait ScoreboardServerHost {
    /// Side: server.
    /// `Scoreboard.getObjective`: the objective with that name, or None. The name is any
    /// string; saved objectives are there from the world's start.
    #[access(WorldOnly)]
    fn objective(&self, name: &str) -> Option<ObjectiveRef>;
    /// Side: server.
    /// `Scoreboard.getObjectives`: every objective. Vanilla keeps them in a hash map, so no
    /// order is promised; Sunrise answers its board's own.
    #[access(WorldOnly)]
    fn objectives(&self) -> Vec<ObjectiveRef>;
    /// Side: server.
    /// `Scoreboard.addObjective`: a new objective with no number format that does not update
    /// scores' display names, shown nowhere. The name is any string, as the board takes it;
    /// the `/scoreboard` command only reads a single word. Refused with
    /// [`ObjectiveError::NameTaken`] or [`ObjectiveError::UnknownCriteria`].
    ///
    /// `display_name` is bounded as every `Text` is: one deeper than 512 component nodes is
    /// refused with [`ObjectiveError::Text`] before any change.
    #[access(WorldOnly)]
    fn add_objective(
        &mut self,
        name: &str,
        criteria: &str,
        display_name: Text,
        render_type: ObjectiveRenderType,
    ) -> Result<ObjectiveRef, ObjectiveError>;
    /// Side: server.
    /// `Scoreboard.getDisplayObjective`: the objective shown in the slot, or None.
    #[access(WorldOnly)]
    fn display_slot_objective(&self, slot: DisplaySlot) -> Option<ObjectiveRef>;
    /// Side: server.
    /// `Scoreboard.setDisplayObjective`: shows the objective in the slot, or clears the slot
    /// with None. Clients are sent the change by the board's usual per-tick sync. False for a
    /// stale objective.
    #[access(WorldOnly)]
    fn set_display_slot(&mut self, slot: DisplaySlot, objective: Option<ObjectiveRef>) -> bool;
}
/// The handles' reads of the `scoreboard` area, implemented by the game and reached through [`Backend`].
// hidden: a handle's backend, reached through `Backend`; mods call the handle, never this trait
#[doc(hidden)]
#[sunrise_mod_api_macros::host_area]
pub trait ScoreboardBackend {
    /// Side: server.
    /// `Objective.getName`, or None for a stale handle.
    #[access(WorldOnly)]
    fn objective_name(&self, objective: ObjectiveRef) -> Option<String>;
    /// Side: server.
    /// `Objective.getCriteria`: its name (`dummy`, `trigger`, `deathCount`, `minecraft.mined:minecraft.stone`...).
    #[access(WorldOnly)]
    fn objective_criteria(&self, objective: ObjectiveRef) -> Option<String>;
    /// Side: server.
    /// `Objective.getDisplayName`.
    #[access(WorldOnly)]
    fn objective_display_name(&self, objective: ObjectiveRef) -> Result<Text, ObjectiveError>;
    /// Side: server.
    /// `Objective.getRenderType`.
    #[access(WorldOnly)]
    fn objective_render_type(&self, objective: ObjectiveRef) -> Option<ObjectiveRenderType>;
    /// Side: server.
    /// `Objective.numberFormat`: None inside the Ok when the objective has none, so the slot's
    /// default shows. A fixed text the API cannot hold is [`ObjectiveError::Text`].
    #[access(WorldOnly)]
    fn objective_number_format(
        &self,
        objective: ObjectiveRef,
    ) -> Result<Option<NumberFormat>, ObjectiveError>;
    /// Side: server.
    /// `Objective.displayAutoUpdate`: whether setting a score copies the holder's display name
    /// into the score.
    #[access(WorldOnly)]
    fn objective_display_auto_update(&self, objective: ObjectiveRef) -> Option<bool>;
    /// Side: server.
    /// The display slots that show the objective, in id order.
    #[access(WorldOnly)]
    fn objective_slots(&self, objective: ObjectiveRef) -> Option<Vec<DisplaySlot>>;
    /// Side: server.
    /// `Scoreboard.getPlayerScoreInfo`: the holder's score, or None when it has none, a holder
    /// that does not resolve (an offline player's entity) or a stale handle. It never creates a score.
    #[access(WorldOnly)]
    fn objective_score(&self, objective: ObjectiveRef, holder: ScoreHolder) -> Option<i32>;
    /// Side: server.
    /// `Scoreboard.listPlayerScores`: every holder's score. Vanilla's order is a hash map's;
    /// the docs promise none, and Sunrise answers the board's own. None for a stale handle.
    #[access(WorldOnly)]
    fn objective_scores(&self, objective: ObjectiveRef) -> Option<Vec<ScoreEntry>>;
    /// Side: server.
    /// `ScoreAccess.locked`, the lock `/trigger` uses: a score created at runtime starts locked,
    /// one loaded from a save without the flag, unlocked. None when there is no score.
    #[access(WorldOnly)]
    fn objective_score_locked(&self, objective: ObjectiveRef, holder: ScoreHolder) -> Option<bool>;
}
/// The handles' writes of the `scoreboard` area, implemented by the game and reached through [`BackendMut`].
// hidden: a handle's backend, reached through `Backend`; mods call the handle, never this trait
#[doc(hidden)]
#[sunrise_mod_api_macros::host_area]
pub trait ScoreboardBackendMut {
    /// Side: server.
    /// `Objective.setDisplayName`: viewers are sent the new name.
    ///
    /// `name` is bounded as every `Text` is: one deeper than 512 component nodes is refused with
    /// [`ObjectiveError::Text`] before any change.
    #[access(WorldOnly)]
    fn objective_set_display_name(
        &mut self,
        objective: ObjectiveRef,
        name: Text,
    ) -> Result<(), ObjectiveError>;
    /// Side: server.
    /// `Objective.setRenderType`. False for a stale handle.
    #[access(WorldOnly)]
    fn objective_set_render_type(
        &mut self,
        objective: ObjectiveRef,
        render_type: ObjectiveRenderType,
    ) -> bool;
    /// Side: server.
    /// `Objective.setNumberFormat`: None goes back to the slot's default.
    #[access(WorldOnly)]
    fn objective_set_number_format(
        &mut self,
        objective: ObjectiveRef,
        format: Option<NumberFormat>,
    ) -> Result<(), ObjectiveError>;
    /// Side: server.
    /// `Objective.setDisplayAutoUpdate`. False for a stale handle.
    #[access(WorldOnly)]
    fn objective_set_display_auto_update(&mut self, objective: ObjectiveRef, auto: bool) -> bool;
    /// Side: server.
    /// `ScoreAccess.set`, creating the score at 0 first (locked). Clients are told only when
    /// something changed. Refused with [`ObjectiveError::Stale`] for a stale handle,
    /// [`ObjectiveError::ReadOnlyCriteria`] for a read-only criteria, and
    /// [`ObjectiveError::UnknownHolder`] for a holder that does not resolve.
    #[access(WorldOnly)]
    fn objective_set_score(
        &mut self,
        objective: ObjectiveRef,
        holder: ScoreHolder,
        value: i32,
    ) -> Result<(), ObjectiveError>;
    /// Side: server.
    /// `ScoreAccess.add`: the new score, wrapping on overflow, with the refusals of `set_score`.
    #[access(WorldOnly)]
    fn objective_add_score(
        &mut self,
        objective: ObjectiveRef,
        holder: ScoreHolder,
        delta: i32,
    ) -> Result<i32, ObjectiveError>;
    /// Side: server.
    /// `Scoreboard.resetSinglePlayerScore`: removes the holder's score. False when it had none.
    #[access(WorldOnly)]
    fn objective_reset_score(&mut self, objective: ObjectiveRef, holder: ScoreHolder) -> bool;
    /// Side: server.
    /// `ScoreAccess.lock` and `unlock`. Clients are never sent the lock. False when the holder has no score.
    #[access(WorldOnly)]
    fn objective_set_score_locked(
        &mut self,
        objective: ObjectiveRef,
        holder: ScoreHolder,
        locked: bool,
    ) -> bool;
    /// Side: server.
    /// `Scoreboard.removeObjective`: its display slots clear, its scores go, and the handle (and
    /// any other for it) is stale for good, even if the name is added again.
    #[access(WorldOnly)]
    fn objective_remove(&mut self, objective: ObjectiveRef) -> bool;
}
/// The client host's methods of the `scoreboard` area, implemented by the game and reached through [`ClientHost`].
#[sunrise_mod_api_macros::host_area]
pub trait ScoreboardClientHost {
    /// Side: client.
    /// What this client has been sent of the scoreboard: the objectives shown in a display
    /// slot, with their scores. None outside a logged-in client world. A text the API cannot hold
    /// (nested deeper than 512 component nodes) refuses the whole snapshot with
    /// [`TextFormError::TooDeep`]; nothing is flattened or skipped.
    #[access(ClientSetup)]
    fn client_scoreboard(&self) -> Result<Option<ClientScoreboard>, TextFormError>;
}
