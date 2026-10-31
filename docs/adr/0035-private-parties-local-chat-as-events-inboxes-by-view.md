# 0035: Parties are private rows; Local chat is a public event by cell; party chat and whispers go to private inboxes

- **Status:** Accepted, 2026-10-31
- **Context:**
  - Party and local chat (BACKLOG M1, docs/design/social.md) finish M1's systems. The original's party held five; its loot modes, experience split and chat ranges weren't found.
  - PLAN §4.4 planned event tables for public chat, and §4.5 a private inbox read through a view for whispers and a per-identity token bucket for rate limits.
  - Event tables keep nothing, and can't be read from views (RESEARCH §3.2), so they suit speech anyone nearby may hear and nothing private.
- **Decision:**
  - **Parties** are private `party` and `party_member` rows. Members read their party through `my_party`, which finds the caller's characters through `character_private` and never reads `presence`, so moves don't re-run it; `party_member.entity_id` is kept by entering and leaving the world instead. Membership outlives leaving the world.
  - **Invitations** are private scheduled rows (`party_invite`) that remove themselves when they lapse; accepting checks lapsing by game time, so the dev clock needs no rescheduling.
  - **Local chat** is a public event row (`local_chat`) in the speaker's cell, subscribed with the Near tier. The client keeps what's said within `local_m` of where it was said. The Near tier always reaches farther than `local_m`, which contentgen holds to 100 m or less.
  - **Party chat and whispers** are copied into private `chat_message` rows, one per recipient (and one for a whisper's sender), read through `my_chat`. Each character keeps its latest `inbox`; the oldest go as new ones come. Party notices (joined, left, leads) are the same rows from nobody.
  - **The rate limit** is a token bucket kept as one timestamp per account in private `chat_rate`, computed by `sim::chat::send`.
  - **A party changes play** in three places only: enemy skills are refused on members, a kill's experience is split among members alive within `share_m` with a bonus per sharer, and the tagger's party counts as one for owning and taking loot.
  - **The client gates every polled key** through `GameInput`, which is quiet while the chat line is open.
- **Consequences:**
  - The schema changed (`party`, `party_member`, `party_invite`, `chat_message`, `chat_rate` and `local_chat`, the `my_party`, `my_party_invites` and `my_chat` views) and content changed (`content/social.toml`): run `cargo xtask publish --clear` and then `cargo xtask world` once.
  - Anyone may subscribe to any cell's `local_chat` and read what's said there, as with positions. Nothing private goes through it.
  - Whispers need the recipient in the world until mail (M2). Members out of the world still get party chat in their inbox.
  - `combat::reward` now takes the victim, for where the kill was; NPC loot checks the party at the time it's taken.
