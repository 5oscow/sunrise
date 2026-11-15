# Client networking

Call `host.disconnect(reason)` through `ClientHost` to leave the joined server or world at the end of the frame, outside hooks. Pass `Some(Text)` to show a disconnect reason or `None` for the normal leave flow. The disconnect screen renders the reason in the player's language, like other translatable text: a translatable component shows its translated text and a plain-text reason is unchanged. The call returns `Result<(), DisconnectError>`: `Unavailable` means no world is joined, it is closing, or a save is pending, and `Text` reports an invalid reason (a malformed or too-deep component is refused by that name and nothing changes). A pending request belongs to its current world epoch and cannot leave a later world. Server code continues to use the separate player disconnect operation.

## Holding outgoing play packets

Call `host.hold_packets()` to start an ordered hold and receive its opaque `HoldId`.
While a hold is active, another `hold_packets()` returns
`HoldError::Limit { packets: 256, bytes: 1_048_576 }` and leaves the first hold intact.
Uncancelled packets from `client_packet_send` queue in creation order;
mod packets accepted by `send_packet` join the queue when their root transaction commits.
`release_held(id)` sends them oldest first and returns the published count. `drop_held(id)`
discards the queue. Neither operation replays the cancellation hook or exposes the packet bodies.

The bounds are **256 packets** and **1,048,576 encoded bytes**, including packet type ids.
Overflow ends the hold and flushes it oldest first, including the packet that exceeded the bound.
Later packets send normally. The next release or drop of that id returns
`HoldError::Limit { packets: 256, bytes: 1_048_576 }`; subsequent calls return `Unknown`.
A failed release returns `HoldError::Send { sent, error }`, using `SendPacketError` for the reason,
and drops the unsent tail.

Disconnect, a protocol incarnation change, or an aborted dimension crossing discards the hold;
its id then returns `Unknown`. Starting without a joined physical play connection returns `NoWorld`.
Network-thread keep-alives, configuration packets, cookies and resource-pack answers bypass holds.
No packet inspection or editing beyond the existing `ClientPacket` view is provided.
