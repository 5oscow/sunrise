//! The window's connection to a server (docs/layers.md, "How a Sunrise crate plugs in"): what
//! [`crate::Hooks`] hears as one starts, lets the player in and ends, and how a crate has the game
//! connect again after one was lost. The engine shows the Disconnected screen, its line and its
//! Cancel button, and makes the connection; the crate decides whether, when and what the line says.

/// A server the window connects to over the network.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Server {
    /// As the player gave it: `host`, `host:port`, an invite code.
    pub address: String,
    /// The player name it connects with.
    pub name: String,
    /// Found on the local network (the server list's LAN worlds).
    pub lan: bool,
}

impl Server {
    pub fn new(address: impl Into<String>, name: impl Into<String>, lan: bool) -> Self {
        Self {
            address: address.into(),
            name: name.into(),
            lan,
        }
    }
}

/// How a connection ended when the player didn't leave ([`crate::Hooks::ended`]).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Ended<'a> {
    /// The reason, as the Disconnected screen shows it, in the player's language.
    pub reason: &'a str,
    /// Its translation key when it had one (`multiplayer.disconnect.kicked`), whatever language
    /// the text is in.
    pub key: Option<&'a str>,
    /// The server, or a refusal on the way in, gave the reason: the connection itself didn't fail.
    pub from_server: bool,
}

impl<'a> Ended<'a> {
    pub fn new(reason: &'a str, key: Option<&'a str>, from_server: bool) -> Self {
        Self {
            reason,
            key,
            from_server,
        }
    }
}

/// What a crate answers while the Disconnected screen offers to connect again
/// ([`crate::Hooks::retry`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Retry {
    /// Not yet: the line the screen shows under the reason, above its Cancel button.
    Wait(String),
    /// Connect to this server now.
    Now(Server),
}
