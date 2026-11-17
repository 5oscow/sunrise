# Read-only level support and sky

`LevelHost` now exposes `block_face_sturdy` and `can_see_sky` through the same engine queries as `LevelRef`. They refuse clients and unloaded positions, never load chunks or unpack loot, and are owning-game-thread reads. No hook version changes.
