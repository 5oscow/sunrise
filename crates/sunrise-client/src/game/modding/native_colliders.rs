//! Moving colliders for native mods (`EntityRef::set_collider`): the boxes an entity carries
//! that every other entity collides with, kept on the server, saved with the entity, and sent
//! to the Sunrise clients that track it.
//!
//! The server keeps a [`ColliderEntry`] by entity id beside the entity's attachments, and a
//! client keeps what its tracking server sent in `Remote::colliders`. A level with no colliders
//! holds empty maps, so nothing else in the game pays for them.
use super::{Dim, Game, commands::Target};
use mod_api::{Collider, ColliderError, EntityRef};
use std::collections::{HashMap, HashSet};
use sunrise_client::{
    colliders::{ColliderState, valid_rotation},
    fast_hash::FxHashMap,
};
use sunrise_core::{
    geometry::Vec3,
    nbt::{Compound, Tag},
};
use sunrise_mod_runtime::attachments::{Attachments, Holder};
use sunrise_protocol::collider_sync::{self, Message};

/// The save key of an entity's collider, beside its attachments.
pub(super) const SAVE_KEY: &str = "SunriseCollider";

/// A server entity's collider, with the form the save writes.
pub(in crate::game) struct ColliderEntry {
    pub state: ColliderState,
    saved: Compound,
}

/// The per-entity maps the game keeps by entity id: attachments, and colliders. It reads as
/// the attachments map, which is what most of the game wants; the colliders are beside it so
/// that what saves, loads and forgets an entity's attachments handles its collider too.
#[derive(Default)]
pub(in crate::game) struct EntityMaps {
    attachments: FxHashMap<u32, Attachments>,
    colliders: FxHashMap<u32, ColliderEntry>,
    /// The ids of the colliders each level holds, so a level's tick places only its own.
    by_level: HashMap<Dim, HashSet<u32>>,
}

impl std::ops::Deref for EntityMaps {
    type Target = FxHashMap<u32, Attachments>;
    fn deref(&self) -> &Self::Target {
        &self.attachments
    }
}

impl std::ops::DerefMut for EntityMaps {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.attachments
    }
}

impl EntityMaps {
    pub fn collider(&self, id: u32) -> Option<&ColliderEntry> {
        self.colliders.get(&id)
    }

    pub fn collider_mut(&mut self, id: u32) -> Option<&mut ColliderEntry> {
        self.colliders.get_mut(&id)
    }

    /// Gives the entity `id`, which stands in `level`, this collider.
    pub fn insert_collider(&mut self, id: u32, level: Dim, entry: ColliderEntry) {
        if self.colliders.insert(id, entry).is_some() {
            self.unindex(id);
        }
        self.by_level.entry(level).or_default().insert(id);
    }

    /// Takes the collider off the entity, wherever it stands.
    pub fn remove_collider(&mut self, id: u32) -> bool {
        let had = self.colliders.remove(&id).is_some();
        if had {
            self.unindex(id);
        }
        had
    }

    /// The entity `id` arrived in `level` with its collider.
    pub fn move_collider(&mut self, id: u32, level: Dim) {
        if self.colliders.contains_key(&id) {
            self.unindex(id);
            self.by_level.entry(level).or_default().insert(id);
        }
    }

    fn unindex(&mut self, id: u32) {
        self.by_level.retain(|_, ids| {
            ids.remove(&id);
            !ids.is_empty()
        });
    }

    /// The ids of the colliders in `level`.
    pub fn colliders_in(&self, level: Dim) -> impl Iterator<Item = u32> + '_ {
        self.by_level.get(&level).into_iter().flatten().copied()
    }

    /// Writes the entity's collider into its saved compound.
    pub(super) fn write_collider(&self, root: &mut Compound, id: u32) {
        match self.colliders.get(&id) {
            Some(entry) => {
                root.insert(SAVE_KEY.into(), Tag::Compound(entry.saved.clone()));
            }
            None => {
                root.remove(&SAVE_KEY.into());
            }
        }
    }
}

/// What the server sends about one entity's collider this tick.
pub(in crate::game) struct ColliderChange {
    entity: u32,
    messages: Vec<Message>,
}

impl Game<'_> {
    /// The entity id a handle names when it may carry a collider: a server entity that is
    /// not a player.
    fn collider_holder(&self, entity: EntityRef) -> Result<u32, ColliderError> {
        if !self.is_logical_server() {
            return Err(ColliderError::StaleEntity);
        }
        match self.native_entity_target(entity) {
            None | Some(Target::Player | Target::Guest(_)) => Err(ColliderError::StaleEntity),
            Some(_) => Ok(entity.id()),
        }
    }

    pub(super) fn entity_collider(&self, entity: EntityRef) -> Option<Collider> {
        let id = self.collider_holder(entity).ok()?;
        self.entity_attachments
            .collider(id)
            .map(|entry| entry.state.to_api())
    }

    pub(super) fn entity_set_collider(
        &mut self,
        entity: EntityRef,
        collider: Option<Collider>,
    ) -> Result<(), ColliderError> {
        let id = self.collider_holder(entity)?;
        let Some(collider) = collider else {
            if self.entity_attachments.remove_collider(id) {
                self.collider_changes.push(ColliderChange {
                    entity: id,
                    messages: vec![Message::Clear { entity: id }],
                });
            }
            return Ok(());
        };
        let state = ColliderState::from_api(&collider)?;
        let messages = match self.entity_attachments.collider(id) {
            Some(old) => {
                let mut messages = collider_sync::changes(id, old.state.wire(), state.wire());
                if old.state.rotation() != state.rotation() {
                    messages.push(Message::Rotation {
                        entity: id,
                        rotation: state.rotation(),
                    });
                }
                messages
            }
            None => state.messages(id),
        };
        let saved = state.to_nbt();
        let level = self.dim;
        self.entity_attachments
            .insert_collider(id, level, ColliderEntry { state, saved });
        if !messages.is_empty() {
            self.collider_changes.push(ColliderChange {
                entity: id,
                messages,
            });
        }
        Ok(())
    }

    pub(super) fn entity_set_collider_transform(
        &mut self,
        entity: EntityRef,
        rotation: [f32; 4],
    ) -> Result<(), ColliderError> {
        let id = self.collider_holder(entity)?;
        if !valid_rotation(rotation) {
            return Err(ColliderError::NonFinite { index: None });
        }
        let Some(entry) = self.entity_attachments.collider_mut(id) else {
            return Err(ColliderError::NoCollider);
        };
        if entry.state.rotation() == rotation {
            return Ok(());
        }
        entry.state.set_rotation(rotation);
        entry.saved = entry.state.to_nbt();
        self.collider_changes.push(ColliderChange {
            entity: id,
            messages: vec![Message::Rotation {
                entity: id,
                rotation,
            }],
        });
        Ok(())
    }

    /// Reads the collider a saved entity carried. A malformed one is dropped with a log line.
    pub(super) fn load_saved_collider(&mut self, id: u32, tag: &Tag) {
        let Some(saved) = tag.get(SAVE_KEY) else {
            return;
        };
        match (saved, ColliderState::from_nbt(saved)) {
            (Tag::Compound(saved), Some(state)) => {
                let level = self.dim;
                self.entity_attachments.insert_collider(
                    id,
                    level,
                    ColliderEntry {
                        state,
                        saved: saved.clone(),
                    },
                );
            }
            _ => log::warn!("Sunrise: dropped a malformed saved collider of entity {id}"),
        }
    }

    /// Where the collider entity `id` is, in the level in place: the server's entity, or a
    /// client's copy of what its server shows it.
    fn collider_origin(&self, id: u32) -> Option<Vec3> {
        let at = if self.is_logical_server() {
            self.target_position(self.level_entity(id as i32)?)?
        } else {
            let epoch = self.native_lifecycle.epoch()?;
            self.native_client_tracked(EntityRef::new(epoch, id))?.1
        };
        Some(Vec3::new(at[0], at[1], at[2]))
    }

    /// Every entity in the vehicle stack the entity `id` is in: its root vehicle and each
    /// passenger at any depth, `id` and the vehicles under it included
    /// (`Entity.isPassengerOfSameVehicle`). On a client, the riding graph its server told.
    pub(super) fn vehicle_stack(&self, id: u32) -> Vec<u32> {
        if self.is_logical_server() {
            let Some(epoch) = self.native_lifecycle.epoch() else {
                return vec![id];
            };
            let entity = EntityRef::new(epoch, id);
            let root = self.entity_root_vehicle(entity).unwrap_or(entity);
            let mut stack = vec![root.id()];
            stack.extend(
                self.entity_indirect_passengers(root)
                    .unwrap_or_default()
                    .iter()
                    .map(|rider| rider.id()),
            );
            if !stack.contains(&id) {
                stack.push(id);
            }
            return stack;
        }
        let Some(remote) = &self.remote else {
            return vec![id];
        };
        let vehicles = &remote.passenger_vehicles;
        // Up to the root, then every entity whose way up reaches it; the walks are bounded by
        // the graph's size, so a cycle in what a server sent ends them.
        let root_of = |mut at: u32| {
            for _ in 0..=vehicles.len() {
                match vehicles.get(&at) {
                    Some(&vehicle) => at = vehicle,
                    None => break,
                }
            }
            at
        };
        let root = root_of(id);
        let mut stack = vec![root];
        stack.extend(
            vehicles
                .keys()
                .copied()
                .filter(|&rider| root_of(rider) == root),
        );
        if !stack.contains(&id) {
            stack.push(id);
        }
        stack
    }

    /// The colliders of the level in place, where they stand now, for the movers of its tick
    /// (`sunrise_client_render::moving_colliders::in_level`): a collider is solid at the
    /// position it had when its level's tick began, and moves after it. A level with none
    /// answers an empty list and pays nothing more.
    pub(in crate::game) fn placed_moving_colliders(
        &self,
    ) -> Vec<sunrise_client_render::moving_colliders::Placed> {
        use sunrise_client_render::moving_colliders::Placed;
        let server = self.is_logical_server();
        let ids: Vec<u32> = if server {
            self.entity_attachments.colliders_in(self.dim).collect()
        } else {
            self.remote
                .as_ref()
                .map_or_else(Vec::new, |r| r.colliders.keys().copied().collect())
        };
        let mut placed = Vec::with_capacity(ids.len());
        for id in ids {
            let state = if server {
                self.entity_attachments.collider(id).map(|e| &e.state)
            } else {
                self.remote.as_ref().and_then(|r| r.colliders.get(&id))
            };
            let (Some(state), Some(origin)) = (state, self.collider_origin(id)) else {
                continue;
            };
            placed.push(Placed {
                collider: state.oriented(origin),
                exempt: self.vehicle_stack(id),
            });
        }
        placed
    }

    /// Sends what changed this tick to the clients that track each entity and have the
    /// channel, except those whose whole collider `fresh` is about to send.
    pub(super) fn flush_collider_sync(&mut self, fresh: &HashSet<(u128, u32)>) {
        let changes = std::mem::take(&mut self.collider_changes);
        for change in changes {
            let senders = self.lan.as_ref().map_or_else(Vec::new, |host| {
                host.hub
                    .guests
                    .iter()
                    .filter(|g| {
                        host.hub.here(g)
                            && g.placed
                            && g.negotiated_collider_sync()
                            && g.tracker.is_tracking(change.entity as i32)
                            && !fresh.contains(&(g.uuid, change.entity))
                    })
                    .map(|g| g.sender.clone())
                    .collect()
            });
            for message in &change.messages {
                self.send_collider_sync(&senders, message);
            }
        }
    }

    /// The whole collider of the entity `id` to one player who started tracking it.
    pub(super) fn send_collider_snapshot(
        &mut self,
        sender: &sunrise_client::lan::conn::PlayerSender,
        id: u32,
    ) {
        let Some(entry) = self.entity_attachments.collider(id) else {
            return;
        };
        let messages = entry.state.messages(id);
        for message in &messages {
            self.send_collider_sync(std::slice::from_ref(sender), message);
        }
    }

    fn send_collider_sync(
        &mut self,
        senders: &[sunrise_client::lan::conn::PlayerSender],
        message: &Message,
    ) {
        use mod_api::{PayloadBytes, PayloadId, PayloadSendError};
        use sunrise_client::raw_payloads::Endpoint;
        let body = collider_sync::encode(message);
        for sender in senders {
            let sent = self.queue_current_payload(
                Endpoint::server(sender.clone()),
                &PayloadId(collider_sync::CHANNEL.into()),
                PayloadBytes(body.clone()),
            );
            match sent {
                Ok(_) => {}
                // A peer without the channel, or between epochs, is sent nothing.
                Err(
                    PayloadSendError::UnsupportedPeer(_)
                    | PayloadSendError::NotNegotiated
                    | PayloadSendError::WrongProtocol(_)
                    | PayloadSendError::StaleEpoch
                    | PayloadSendError::Disconnected,
                ) => {}
                // The replica would miss the change: close, as a full queue always did.
                Err(PayloadSendError::Backpressure) => sender.close(),
                Err(error) => log::warn!("Sunrise: collider sync not sent: {error:?}"),
            }
        }
    }

    /// A `sunrise:collider_sync` payload the server sent the window.
    pub(super) fn remote_collider_sync(&mut self, data: &[u8]) {
        let message = match collider_sync::decode(data) {
            Ok(message) => message,
            Err(e) => return log::debug!("Sunrise: a bad collider sync: {e}"),
        };
        let Some(remote) = self.remote.as_mut() else {
            return;
        };
        let entity = match message {
            Message::Boxes { entity, .. }
            | Message::Patch { entity, .. }
            | Message::Rotation { entity, .. }
            | Message::Clear { entity } => entity,
        };
        let mut held = remote.colliders.remove(&entity);
        ColliderState::apply(&mut held, &message);
        if let Some(state) = held {
            remote.colliders.insert(entity, state);
        }
    }

    /// A client forgets the collider of an entity the server stopped showing it.
    pub(super) fn forget_client_collider(&mut self, id: i32) {
        if let Some(remote) = self.remote.as_mut() {
            remote.colliders.remove(&(id as u32));
        }
    }

    /// The holders a flush sends whole collider snapshots for: the entities players started
    /// to track. Marks the pairs so the change flush skips them.
    pub(super) fn fresh_collider_pairs(snapshots: &[(u128, Holder)]) -> HashSet<(u128, u32)> {
        snapshots
            .iter()
            .filter_map(|(player, holder)| match holder {
                Holder::Entity(id) => Some((*player, *id)),
                _ => None,
            })
            .collect()
    }
}
