# Social: party and chat

Status: v0.1, 2026-10-31. M1 task 1 (BACKLOG M1 #1). Code: `content/social.toml`, `server/sim/src/party.rs` and `chat.rs`, `server/world/src/party.rs` and `chat.rs` (hooks in `session.rs`, `combat.rs`, `npc.rs`); `sidecars/loadbot/src/party.rs`, and `visitor.rs`, which answers; client `client/src/Sim/Chat.cs`, `Gameplay/Party.cs`, `Gameplay/ChatModel.cs`, `Gameplay/GameInput.cs`, `UI/ChatBox.cs`, `UI/PartyFrame.cs`, `UI/PartyDemo.cs`. Related: docs/design/combat.md (experience), docs/design/npcs.md (loot), ADR 0035.

M1 covers **a party and Local, party and whisper chat** (PLAN §11). Raids, guilds, families, mail and the other channels come later (PLAN §9: M2 for guild, family and mail).

## 1. How the original worked

| Fact | Confidence | Source |
|---|---|---|
| A party held up to **five** players. A raid joined several parties | B | [ArcheAge Wiki: Group](https://archeage.fandom.com/wiki/Group) (via search, 2026-10-31; the page didn't load); [Steam discussion: party size](https://steamcommunity.com/app/304030/discussions/0/618457398968554809) |
| Party members shared quest objectives, and an automatic system divided up what enemies dropped | B | [Gamepressure guide: guilds, parties, raids](https://guides.gamepressure.com/archeage/guide.asp?ID=27113) (via search) |
| Raid members showed as small squares in the raid frame | C | same |
| Chat channels: **Local** for anyone physically near, Shout and LFG limited to the zone, Faction, Nation and Trade everywhere, Guild and Family for their members | B | [ArcheAge Wiki: Interface](https://archeage.fandom.com/wiki/Interface) (via search) |
| Slash commands: `/s` Local, `/w name` whisper, `/g` guild, `/f` faction, `/tr` trade, `/sh` shout, `/fam` family, `/lfg`; keys to reply to a whisper | B | [official forum archive: how to whisper](http://forums.archeagegame.com/showthread.php?244224-How-to-you-Whisper-and-unWhisper=) (via search, about 2015) |
| The raid size, the loot modes and their grade thresholds, how far experience was shared and how it was split, Local chat's range, and any rate limit | **Unknown** | not found |

## 2. Our rules

### Content (`content/social.toml`)
- `[party]`: `size` (5, B), `invite_s` (60), `share_m` (100), `bonus_pct` (10).
- `[chat]`: `local_m` (40), `max_chars` (200), `burst` (5), `refill_ms` (2,000), `inbox` (50).
- Contentgen checks the size is 2 to 50, invitations last 5 s to an hour, the share distance is 1 to 1,000 m, the bonus 0 to 100%, Local's range 1 to 100 m (it must stay inside the Near tier, §2 Chat), messages 1 to 1,000 characters, the burst 1 to 100, the refill 50 ms to a minute and the inbox 1 to 1,000. All of it is part of the content hash.

### Parties
- A private **`party`** row per party (`id`, `made_at`), and a private **`party_member`** row per member: `char_id`, `party_id`, `name`, `entity_id` (its entity in the world, or 0 while it's out), `leader`, `joined_at`. Members read their party's rows through **`my_party`**.
- A private **`party_invite`** row per invitation (`from_char`, `from_name`, `to_char`, `to_name`, `made_at`). It's a scheduled row that removes itself at `made_at + invite_s` (`lapse_party_invite`). Both ends read it through **`my_party_invites`**. Accepting checks the time by game time, so the dev clock lapses invitations at once.
- **`invite_to_party(name)`**: the caller is in the world ("not in the world"); a character of that name, any case, is in the world ("no one called X is in the world"), isn't the caller ("you can't invite yourself") and is in no party ("X is already in a party"). A caller in a party must lead it ("only the party's leader can invite") and have room ("your party is full"). A second invitation from the same character to the same one replaces the first.
- **`accept_party_invite(id)`**: an invitation to the caller's character ("no such invitation") that hasn't lapsed ("that invitation has lapsed"), and the caller isn't in a party ("you're already in a party"). If the inviter is in a party it must still lead it ("X no longer leads a party") and have room ("X's party is full"); if it's in none, the party is made now with the inviter as leader. Joining removes every other invitation to the caller.
- **`decline_party_invite(id)`**: by either end; it goes.
- **`leave_party`**, **`kick_from_party(char_id)`** (the leader, of anyone else in the party, in the world or not) and **`make_party_leader(char_id)`** (the leader, to another member). A leader who goes passes the lead to the oldest member left. A party left with one member breaks up.
- Leaving the world keeps a character in its party, with `entity_id` 0, and removes its invitations, both ways. Entering again sets `entity_id`.
- The party tells its members what happens, as **notices** in party chat (from nobody): "X joined the party", "X left the party", "X was removed from the party", "X leads the party", and to the one going "you left the party", "you were removed from the party" or "the party has broken up".

### What a party does
- **No harm:** a skill aimed at an enemy is refused on a member of the caller's party ("you can't attack a party member"). Healing and buffs work on anyone, as before. So there's no crime between members.
- **Experience:** a kill that earns experience (docs/design/combat.md) is shared by the earner and every member of its party in the world, alive and within `share_m` of the kill horizontally. Each sharer gets its own `kill_xp` for the kill (by its level) × (100 + `bonus_pct` × (sharers − 1))% ÷ sharers, rounded down. Alone, that's all of it; two get 55% each, five 28%.
- **Loot:** an NPC's tagger owns its loot if the tagger's whole party dealt enough of the damage, counted together (docs/design/npcs.md). Any member of the owner's party may take loot it owns, as the owner can.

### Chat
- **Local:** `chat(0, text)`. A public **event** row in **`local_chat`** (Near tier, by cell): `cell`, `entity_id`, `char_id`, `name`, the world position it was said at, and `text`. It lives only in its transaction, so nothing is kept. Clients hear it if they're within `local_m` of where it was said, horizontally; the Near tier is at least two 64 m cells less the window's 8 m margin in every direction, so everyone in range gets the row. Anyone who subscribes to a cell can read what's said in it: Local chat is public speech, like positions.
- **Party:** `chat(1, text)`, by a member ("you're not in a party"). A copy goes into every member's inbox, in the world or not.
- **Whisper:** `whisper(name, text)` to a character in the world ("no one called X is in the world"; "you can't whisper to yourself"). A copy goes into the recipient's inbox and one, naming the recipient, into the sender's.
- **Inboxes:** private **`chat_message`** rows (`owner`, `char_id`, `channel`, `from_char`, `from_name`, `to_name`, `text`, `sent_at`), read through **`my_chat`**. Each character keeps its latest `inbox` messages; older ones go as new ones come.
- **Messages:** trimmed; refused if empty ("say something"), longer than `max_chars` characters ("that message is too long") or holding control characters such as a newline ("messages are one line of text").
- **Rate limit:** each account has a token bucket of `burst` messages refilling one every `refill_ms`, kept as one timestamp in the private **`chat_rate`** table (the time the bucket is full again), computed by `sim::chat::send`, never ticked (PLAN §4.5). A refused message spends nothing. Over the limit: "you're sending messages too fast".
- Other channels (shout, trade, faction, guild, family) come with their systems.

### The client
- **The chat box**, bottom left: the last 12 lines, coloured by channel (Local white, Party blue, whispers pink, notices yellow, refusals red). Enter, or `/`, opens the line; Enter sends, Escape drops it. While it's open the game's keys go quiet (`GameInput`, which every polled key now goes through).
- Plain lines go to the current channel, Local until `/p` alone switches to Party (`/s` alone switches back). Commands: `/s text`, `/p text`, `/w Name text`, `/r text` (to the last character who whispered to us), `/invite Name`, `/accept`, `/decline` (the newest standing invitation), `/leave`, `/kick Name`, `/lead Name`, `/help`. The client checks a message as the server does before sending, and shows every server refusal in red.
- **The party frame**, top right: "Party (n/5)", then each member with a star for the leader and its level and HP when it's in view, "far" when it's in the world out of view, "away" when it isn't. An invitation to us shows below it with its seconds left.
- Party members' name tags are green.
- `--party-demo` (dev builds, with `bot visitor` nearby, which takes invitations and answers what's said to it): invites the visitor, then says hello aloud, in the party and in a whisper, waits for each answer, replies with `/r` and leaves, typing each line into the box.

## 3. Deviations and not yet done
- **Only Local, party and whisper chat.** No shout, trade or faction channels until zones and factions exist, nor guild and family chat until those do (M2).
- **No raids, no loot modes.** The original's loot rules and raid sizes weren't found. Ours: the party shares the owner's loot rights, free for all among members; the first to take it gets it.
- **Experience sharing is ours**: the distance, the bonus and the even split.
- **Whispers need the recipient in the world.** Mail to the offline is M2.
- **Local chat's range is ours (40 m),** and it's heard through walls.
- **No speech bubbles, no chat tabs, no ignoring, no reporting players.** No profanity filter.
- **No party markers on the map,** and no party frames for members' mana or statuses.
- **The rate limit is per account,** and applies to all chat alike.

## 4. Test plan

| Layer | Test | Where |
|---|---|---|
| contentgen | `social.toml` reaches both outputs and the hash; out-of-range values are reported together; a missing section is reported by file | `server/contentgen` |
| sim | invitations lapse at the boundary; sharing range is horizontal; shares add the bonus and split; channels round-trip; messages trimmed and refused (empty, control, too long by characters); the bucket's burst, refill and refilling; Local's range | `server/sim/src/party.rs`, `chat.rs` |
| module | invitations: made, replaced, accepted only by the invitee, every refusal, lapsing (by game time and the scheduler, which players can't run), declining by either end, gone on leaving the world, others gone on joining; a full party; a former leader's invitation; kicks, hand-overs, a leaving leader's successor, breaking up, notices; members kept out of the world; `my_party` and `my_party_invites` show only our own. No harm between members; experience shared by the alive and near only, and all of it alone; a party's damage tags together and members loot. Local as an event at the world position; every message refusal; the rate limit per account; party chat to every member, in the world or not; whispers to one with the sender's copy, and their refusals; an inbox keeps the latest `inbox`; `my_chat` shows only our own | `server/world/src/tests/party.rs`, `chat.rs`, `npc.rs` |
| C# | rules from content; cleaning as the server does (characters, not UTF-16 units); Local's range; lapsing and the invitation line; every command, plain lines by channel and switching; whispers and `/r`; log lines by channel; the frame's member lines | `client/tests/SocialTests.cs` |
| scenario | `loadbot party`: A invites B, who declines, lets one lapse by the dev clock, then accepts; B can't invite; A invites C, 300 m off. A can't harm B. A's Local chat reaches B in range and not C; C's party message reaches all three; A's whisper to C reaches only C and A. A runs into the rate limit. A hands the lead to B, B kicks C, A leaves and the party breaks up. Pass: every step seen | `cargo xtask bot party` |
| client | `--party-demo` against a live server with `bot visitor`, with captures of the chat box, the party frame and green name tags | manual, with a capture |
