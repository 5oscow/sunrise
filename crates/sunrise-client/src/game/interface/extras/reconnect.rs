//! Auto-reconnect (`autoReconnect`, docs/extras.md): the window's connection
//! (`game/network/remote/connection.rs`) tells the tracker (`sunrise_client::reconnect`) as it
//! starts, lets the player in and ends, and the extra answers the Disconnected screen as a crate's
//! `Hooks::retry` would: the countdown, then the attempt.
use crate::game::Game;
use std::time::Instant;
use sunrise_hooks::connection::{Retry, Server};

impl Game<'_> {
    /// Auto-reconnect: a connection to `address` as `name` starts.
    pub(in crate::game) fn extras_connecting(&mut self, address: &str, name: &str, lan: bool) {
        self.extras.retry = None;
        self.extras.reconnect.connecting(address, name, lan);
    }
    /// Auto-reconnect: the server let the player in.
    pub(in crate::game) fn extras_joined(&mut self) {
        self.extras.reconnect.joined(Instant::now());
    }
    /// Auto-reconnect: the player left the server on purpose.
    pub(in crate::game) fn extras_left(&mut self) {
        self.extras.retry = None;
        self.extras.reconnect.reset();
    }
    /// Auto-reconnect: the connection ended; the next attempt when it is on and the reason
    /// allows one.
    pub(in crate::game) fn extras_ended(
        &mut self,
        reason: &str,
        key: Option<&str>,
        from_server: bool,
    ) {
        self.extras.retry = if self.settings.extras.auto_reconnect {
            let now = Instant::now();
            self.extras.reconnect.ended(reason, key, from_server, now)
        } else {
            self.extras.reconnect.reset();
            None
        };
    }
    /// Auto-reconnect's countdown on the Disconnected screen, then its attempt.
    pub(in crate::game) fn extras_retry(&self) -> Option<Retry> {
        let retry = self.extras.retry.as_ref()?;
        let now = Instant::now();
        Some(if retry.due(now) {
            Retry::Now(Server::new(
                retry.address.as_str(),
                retry.name.as_str(),
                retry.lan,
            ))
        } else {
            Retry::Wait(retry.text(now))
        })
    }
}
