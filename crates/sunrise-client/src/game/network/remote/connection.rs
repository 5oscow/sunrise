//! The window's connection to a server, as the engine sees it: its record of the last server, the
//! hooks it calls as a connection starts, lets the player in and ends (`sunrise_hooks::connection`),
//! and the Disconnected screen's offer to connect again, which a crate above asks for
//! (`Hooks::retry`). With nothing asking, the screen is vanilla's `DisconnectedScreen`.
use super::*;
use sunrise_client::{multiplayer, title::Mode};
use sunrise_hooks::connection::{Ended, Retry, Server};

impl Game<'_> {
    /// A connection to `address` as `name` starts over the network (`Hooks::connecting`).
    pub(in crate::game) fn connection_starts(&mut self, address: &str, name: &str, lan: bool) {
        let server = Server::new(address, name, lan);
        sunrise_hooks::engine::connecting(&server);
        self.last_connection = Some(server);
    }
    /// The server this window connected to last over the network: while connected, and after a
    /// lost connection while the Disconnected screen may connect there again.
    pub(in crate::game) fn server_address(&self) -> Option<&str> {
        let server = self.last_connection.as_ref();
        server.map(|server| server.address.as_str())
    }
    /// That server was found on the local network.
    pub(in crate::game) fn server_is_lan(&self) -> bool {
        self.last_connection
            .as_ref()
            .is_some_and(|server| server.lan)
    }
    /// The server let the player in (`Hooks::joined`).
    pub(in crate::game) fn connection_joined(&self) {
        sunrise_hooks::engine::joined();
    }
    /// The player left on purpose: nothing to connect again to (`Hooks::left`).
    pub(in crate::game) fn connection_left(&mut self) {
        self.last_connection = None;
        sunrise_hooks::engine::left();
    }
    /// The connection ended without the player leaving (`Hooks::ended`), before the Disconnected
    /// screen shows. The last server is forgotten unless something will connect there again.
    pub(in crate::game) fn connection_ended(
        &mut self,
        reason: &str,
        key: Option<&str>,
        from_server: bool,
    ) {
        sunrise_hooks::engine::ended(&Ended::new(reason, key, from_server));
        if sunrise_hooks::engine::retry().is_none() {
            self.last_connection = None;
        }
    }
    /// The Disconnected screen just shown offers to connect again while something asks to.
    pub(in crate::game) fn offer_retry(&mut self) {
        let Some(answer) = sunrise_hooks::engine::retry() else {
            return;
        };
        // A connection due at once is made on the next frame (`follow_retry`).
        let line = match answer {
            Retry::Wait(line) => line,
            _ => String::new(),
        };
        let Some(slot) = retry_slot(&mut self.title) else {
            return;
        };
        if let Some(server) = &self.last_connection {
            let address = sunrise_client::net::shown_address(&server.address);
            log::info!("Sunrise: offering to connect to {address} again: {line}");
        }
        *slot = Some(line);
    }
    /// Once a frame while the Disconnected screen offers to connect again: its line, then the
    /// connection once it is due. An online-mode server gets the signed-in account again:
    /// `connect_server` takes it from `auth::current()`, held in memory for the whole run (the
    /// launcher's session file is gone after the first read).
    pub(in crate::game) fn follow_retry(&mut self) -> Result<()> {
        if retry_slot(&mut self.title).is_none_or(|slot| slot.is_none()) {
            return Ok(());
        }
        let answer = sunrise_hooks::engine::retry();
        let Some(slot) = retry_slot(&mut self.title) else {
            return Ok(());
        };
        let server = match answer {
            Some(Retry::Wait(line)) => {
                *slot = Some(line);
                return Ok(());
            }
            Some(Retry::Now(server)) => server,
            // Nothing asks any more.
            _ => {
                *slot = None;
                return Ok(());
            }
        };
        *slot = None;
        let realm = self.realm_session;
        self.connect_server_with_options(&server.address, &server.name, None, server.lan, realm)
    }
}

/// The Disconnected screen's line about connecting again, while that screen shows.
fn retry_slot(title: &mut Option<sunrise_client::title::Title>) -> Option<&mut Option<String>> {
    match &mut title.as_mut()?.mode {
        Mode::Multiplayer(multiplayer::Screen::Disconnected { retry, .. }) => Some(retry),
        _ => None,
    }
}
