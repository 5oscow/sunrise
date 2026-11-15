# Sunrise governance

> **Status: proposed.** This replaces the governance text Sunrise inherited from Horizon. It takes effect when the
> Core Team adopts it by a two-thirds vote, and from then on only that vote changes it.

Sunrise has no boss, and nobody is paid from its money. A small **Core Team** decides the few things that affect who
controls the project and its money. **Area teams** run the work. Everything else is ordinary review.

## Who decides what

| Decision | Who | How |
| --- | --- | --- |
| Code, reviews, releases, day-to-day running | Area teams and reviewers | Pull request review and the required checks |
| An area's direction | The area's lead | Accountable to the area team, which can replace its lead by majority |
| This document and [FUNDING.md](FUNDING.md) | Core Team | Two thirds of all members |
| Spending (outside an approved budget), members, legal action, deals with other projects or companies, control of the organisation, domains, keys and accounts | Core Team | A majority of all members, open for 7 days or until that majority is reached |

Contributors keep the copyright to their code, and Sunrise never relicenses it ([LICENSING.md](LICENSING.md)). No
vote changes that.

No single person can spend Sunrise's money or make a deal on its behalf. Every Core decision is recorded publicly
with each member's vote, except where that would expose a security problem or someone's personal details; those are
recorded privately and summarised publicly once that is safe.

A member doesn't vote on anything that pays them, and says so in the vote's record.

## The Core Team

- Current members: umceko, Isloka, ari and Xmilon.
- Members join by Core vote. Anyone can leave at any time by saying so.
- Removing a member takes a majority of all members. The vote runs the full 7 days, the person concerned can answer
  in the record, and they don't vote.
- In an emergency any member may act at once, but only to revoke or rotate a leaked key or token, or to lock a
  compromised account. They open a vote to confirm or undo it within 48 hours. This never removes another member's
  access or adds anyone.

## Area teams

- Each area (for example the mod API, the launcher, the site, the bot, infrastructure) has a team and one lead.
- Roles are earned with work people can see: merged pull requests, reviews, triage and support.
- An area team can replace its lead by majority.
- The **council** is the Core Team plus the area leads. It coordinates across areas and is where cross-area
  decisions are discussed. The matters in the table above stay with the Core Team's vote.

## Money

The rules are in [FUNDING.md](FUNDING.md): one public pot at a fiscal host, every income and expense public,
infrastructure first, and no salaries, fees or profit shares for anyone. The Core Team plans a non-profit association
as Sunrise's legal home, with these rules in its statutes.

## Accounts and keys

Organisation owners: umceko, Isloka and ari. More owners only by Core vote.

Today some accounts are held by one person: the release signing key and the site's registry are umceko's. The aim is
at least two Core admins on every account and key; each one that gets a second admin is listed here.

## Safeguards from adoption

- `main` takes changes only through pull requests with passing checks. No force pushes or deletion.
- Bots and agents never replace these checks: a bot's mistake or an injected message can't land code alone.

## Rules that are never voted on

These are conditions for Sunrise existing, not choices:

- Playing needs a **Microsoft account that owns Minecraft: Java Edition**.
- **No Mojang files** in Sunrise's repositories or downloads. Players' own copies stay on their own machines.
- **No gambling**, in any form, including with in-game currency.
- **Mojang's and Microsoft's terms and Usage Guidelines** apply to everything Sunrise does.
- **The law**, and the terms of service of Discord and GitHub.

## Licence

Sunrise's own code is under the PolyForm Shield License 1.0.0 ([LICENSE](LICENSE), explained in
[LICENSING.md](LICENSING.md)). Code from Horizon and from arismoko/horizon stays under MIT with its notices
([LICENSES/](LICENSES/)). Contributions come in under the same licence they go out under, signed off under the
Developer Certificate of Origin ([CONTRIBUTING.md](CONTRIBUTING.md)).
