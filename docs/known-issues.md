# Known issues

Every game release lists the bullets below under "Known issues" (tools/release_train.py). Keep them short and
written for players; remove a bullet in the pull request that fixes it.

- Intel macOS and musl Linux archives are unavailable: their builds fail the startup check with `GuestStackUnavailable`.
- Online sign-in waits on Mojang's approval of Sunrise's Microsoft app.
