# Known issues

Every game release lists the bullets below under "Known issues" (tools/release_train.py). Keep them short and
written for players; remove a bullet in the pull request that fixes it.

- Intel macOS and musl Linux archives are unavailable: their builds fail the startup check with `GuestStackUnavailable`.
- Until Mojang approves Sunrise's own Microsoft app, the launcher signs in through the Starling launcher's approved
  app, with its owner's permission: Microsoft's sign-in page names Starling.
