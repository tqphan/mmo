//! The `quests` scenario (docs/design/quests.md): bot A does every coast quest, with dev help, on
//! the world's housing land or on test ground it uploads (as the `farm` scenario does). The four
//! givers, two stations and a Saltreed node are placed round the site, and two posts on the trade
//! island far enough off to buy what's made there. A is refused a quest above its level, one whose
//! chain isn't done and one from the wrong giver; it gathers and crafts Reed and Twine to half and
//! turns it in early for half the experience; it kills Shore Skitters with bot B in its party
//! nearby, both counting, and turns in at half again for half again the experience; it plants and
//! harvests by the dev clock, crafts and sells a Grain Bale and is given a skiff, sells another at
//! Gullhaven Post for the harbourmaster, runs the Cairnwick errand and the camps (at top level),
//! and ends with all ten turned in and taking one again refused. Then it cleans up.

use sim::cell::{OPEN_WORLD, cell_of};
use sim::quest::{GIVERS, QuestData, cap, giver_by_id, quest_by_id, xp_for};

use crate::farm::plant;
use crate::gather::{advance, call, held, set_labor, subscribe};
use crate::land::{OWN_CELLS, grant, refused, upload_own, world_site};
use crate::module_bindings::*;
use crate::npc::{alive, summon, xp_of};
use crate::trade::place_post;
use crate::{Args, Bot, Check, wait_for};
use spacetimedb_sdk::{DbContext, Table};

/// `entity.kind` for NPCs.
const NPC_KIND: u8 = 4;
/// On the trade island, far from any housing land: posts there buy what's made at the site.
const ISLE_POSTS: [(&str, f32, f32); 2] = [("redfen_post", 450.0, 30.0), ("gullhaven_post", -150.0, 30.0)];

fn quest(id: &str) -> Result<&'static QuestData, String> {
    quest_by_id(id).ok_or_else(|| format!("no quest {id} in content"))
}

/// `bot`'s log entry for `id`.
fn entry(bot: &Bot, id: &str) -> Option<QuestLog> {
    let w = quest_by_id(id)?.wire;
    bot.conn.db.my_quests().iter().find(|e| e.quest == w)
}

fn accept(bot: &Bot, giver: u64, id: &str) -> Check {
    let w = quest(id)?.wire;
    call(bot, |tx| bot.conn.reducers.accept_quest_then(giver, w, tx)).map_err(|e| format!("{}: take {id}: {e}", bot.label))?;
    wait_for(id, || entry(bot, id).is_some())
}

fn turn_in(bot: &Bot, giver: u64, id: &str) -> Check {
    let w = quest(id)?.wire;
    call(bot, |tx| bot.conn.reducers.turn_in_quest_then(giver, w, tx)).map_err(|e| format!("{}: turn in {id}: {e}", bot.label))?;
    wait_for(id, || entry(bot, id).is_some_and(|e| e.turned_in_at.is_some()))
}

/// Wait for objective `k` of `id` to reach `n`.
fn reaches(bot: &Bot, id: &str, k: usize, n: u32) -> Check {
    wait_for(&format!("{id} objective {k} at {n}"), || entry(bot, id).is_some_and(|e| e.done.get(k).copied().unwrap_or(0) >= n))
        .map_err(|_| format!("{}: {id} stands at {:?}, wanted objective {k} at {n}", bot.label, entry(bot, id).map(|e| e.done)))
}

/// Turn in `id` and check the experience it paid at `score`.
fn turn_in_for(bot: &Bot, giver: u64, id: &str, score: u32) -> Check {
    let (level, xp) = xp_of(bot);
    let want = sim::combat::gain_xp(level, xp, xp_for(quest(id)?.xp, score));
    turn_in(bot, giver, id)?;
    wait_for("the experience", || xp_of(bot) == want).map_err(|_| format!("{}: {id} left it at {:?}, expected {want:?}", bot.label, xp_of(bot)))?;
    println!("{}: turned in {id} at {score}%: level {} with {} experience", bot.label, want.0, want.1);
    Ok(())
}

/// Summon and kill `n` of `kind` just north of `bot`; returns them for cleaning up.
fn kill_n(bot: &Bot, kind: &str, n: u32, summoned: &mut Vec<u64>) -> Check {
    let p = bot.presence().ok_or("left the world")?;
    for _ in 0..n {
        let id = summon(bot, kind, p.x, p.z - 2.5)?;
        summoned.push(id);
        slay(bot, id)?;
    }
    Ok(())
}

/// Hit `target` until it dies, stepping up to it, turning to face it and healing up before each
/// blow, since one that fights back moves round.
fn slay(bot: &Bot, target: u64) -> Check {
    let started = std::time::Instant::now();
    while alive(bot, target) {
        if started.elapsed() > std::time::Duration::from_secs(60) {
            return Err(format!("{target} is still alive after 60 s"));
        }
        let (Some(me), Some(t)) = (bot.presence(), bot.sees(target)) else { continue };
        let (mut x, mut z) = (me.x, me.z);
        if (t.x - x).hypot(t.z - z) > 2.5 {
            (x, z) = (t.x, t.z + 2.0);
            bot.teleport(x, z)?;
        }
        // Dev help: full HP and mana for every blow, so a pack of wolves can't wear us down.
        call(bot, |tx| bot.conn.reducers.dev_set_vitals_then(i32::MAX, i32::MAX, tx))?;
        let me = bot.presence().ok_or("left the world")?;
        let yaw = sim::ship::yaw_to_u16((-(t.x - x)).atan2(-(t.z - z)));
        call(bot, |tx| bot.conn.reducers.move_event_then(me.last_seq + 1, 0, x, me.y, z, yaw, 0, 1, tx))?;
        for skill in ["rending_blow", "shield_bash", "threefold_cut"] {
            let w = sim::combat::skill_by_id(skill).ok_or("no skill")?.wire;
            if call(bot, |tx| bot.conn.reducers.use_skill_then(w, target, tx)).is_ok() {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(sim::combat::COMBAT.gcd_ms as u64 + 30));
    }
    Ok(())
}

fn station(bot: &Bot, id: &str, x: f32, y: f32, z: f32) -> Result<u64, String> {
    let kind = sim::craft::station_by_id(id).ok_or_else(|| format!("no station {id}"))?;
    call(bot, |tx| bot.conn.reducers.admin_put_stations_then(vec![StationPlacement { kind: kind.wire, x, y, z, yaw: 0 }], tx))?;
    let fresh = || bot.conn.db.station().iter().find(|s| s.x == x && s.z == z && s.kind == kind.wire).map(|s| s.id);
    wait_for(id, || fresh().is_some())?;
    fresh().ok_or_else(|| format!("the {id} vanished"))
}

/// Craft `count` of recipe `id` at `station` and wait until they're made.
fn craft(bot: &Bot, station: u64, id: &str, count: u32) -> Check {
    let r = sim::craft::recipe_by_id(id).ok_or_else(|| format!("no recipe {id}"))?;
    call(bot, |tx| bot.conn.reducers.craft_then(station, r.wire, count, tx)).map_err(|e| format!("craft {id}: {e}"))?;
    std::thread::sleep(std::time::Duration::from_secs_f32(r.craft_s * count as f32));
    wait_for("the craft", || bot.conn.db.my_craft().count() == 0)
}

pub fn scenario(args: &Args) -> Check {
    let a = Bot::connect(args, "A")?;
    a.subscribe_self()?;
    call(&a, |tx| a.conn.reducers.dev_set_gm_then(3, tx))?;
    // The world's housing land when the server has the world; otherwise our own test ground, even
    // if other scenarios (or a failed run) left some land behind.
    let own = crate::trade::Ground::of(&a).is_err() || a.server_terrain().land_chunks == 0;
    let (sx, sz) = if own { upload_own(&a)? } else { world_site(&sim::land::PLOTS[1])? };
    println!("site: ({sx:.0}, {sz:.0}) on {}", if own { "our own test ground" } else { "the world's housing land" });
    let b = Bot::connect(args, "B")?;
    b.subscribe_self()?;
    let mut cells: Vec<u64> = sim::cell::window_cells(cell_of(OPEN_WORLD, sx, sz), 1);
    for &(_, x, z) in &ISLE_POSTS {
        cells.push(cell_of(OPEN_WORLD, x, z));
    }
    for bot in [&a, &b] {
        subscribe(bot, &["SELECT * FROM my_quests", "SELECT * FROM my_fighter", "SELECT * FROM my_labor", "SELECT * FROM game_clock", "SELECT * FROM my_craft", "SELECT * FROM my_pack", "SELECT * FROM my_party", "SELECT * FROM my_party_invites"])?;
        for c in &cells {
            subscribe(
                bot,
                &[
                    &format!("SELECT * FROM quest_giver WHERE cell = {c}"),
                    &format!("SELECT * FROM station WHERE cell = {c}"),
                    &format!("SELECT * FROM trade_post WHERE cell = {c}"),
                    &format!("SELECT * FROM resource_node WHERE cell = {c}"),
                    &format!("SELECT * FROM crop WHERE cell = {c}"),
                    &format!("SELECT * FROM entity WHERE cell = {c}"),
                    &format!("SELECT * FROM vitals WHERE cell = {c}"),
                    &format!("SELECT * FROM transform WHERE cell = {c}"),
                ],
            )?;
        }
        bot.arrive()?;
        call(bot, |tx| bot.conn.reducers.dev_set_gm_then(3, tx))?;
        set_labor(bot, 5000)?;
    }
    let pa = a.presence().ok_or("A left")?;
    a.teleport(sx, sz)?;
    b.teleport(sx + 1.0, sz + 1.0)?;
    let y = a.presence().ok_or("A left")?.y;
    // NPCs a failed run summoned here go first: some bite.
    std::thread::sleep(std::time::Duration::from_millis(500));
    for e in a.conn.db.entity().iter().filter(|e| e.kind == NPC_KIND).collect::<Vec<_>>() {
        let _ = call(&a, |tx| a.conn.reducers.dev_remove_npc_then(e.id, tx));
    }

    // The givers in a ring round the site, the benches and a node beside it, the posts out on the isle.
    let mut givers = Vec::new();
    for (k, g) in GIVERS.iter().enumerate() {
        let (x, z) = (sx + 2.0 * (k as f32 - 1.5), sz + 2.0);
        call(&a, |tx| a.conn.reducers.admin_put_givers_then(vec![GiverPlacement { kind: g.wire, x, y, z, yaw: 0 }], tx))?;
        wait_for(g.id, || a.conn.db.quest_giver().iter().any(|q| q.x == x && q.z == z))?;
        givers.push(a.conn.db.quest_giver().iter().find(|q| q.x == x && q.z == z).ok_or("no giver")?.id);
    }
    let giver = |id: &str| giver_by_id(id).map(|g| givers[usize::from(g.wire)]).ok_or_else(|| format!("no giver {id}"));
    let (odda, brannoch, yeva, quill) = (giver("odda_fernwright")?, giver("brannoch_hale")?, giver("yeva_mossgrove")?, giver("quill_ambry")?);
    let joiner = station(&a, "joiners_bench", sx - 3.0, y, sz)?;
    let packwright = station(&a, "packwright_bench", sx + 3.0, y, sz)?;
    let reed = sim::gather::node_by_id("saltreed_clump").ok_or("no saltreed_clump")?;
    let node = crate::gather::place(&a, reed, sx, y, sz - 1.5)?;
    let mut posts = Vec::new();
    // Each post at the height we stand at beside it, whatever ground the server has there.
    for &(id, x, z) in &ISLE_POSTS {
        a.teleport(x + 1.0, z)?;
        let y = a.presence().ok_or("A left")?.y;
        // A failed run's post here would be found in place of ours.
        let stale: Vec<u64> = a.conn.db.trade_post().iter().filter(|p| p.x == x && p.z == z).map(|p| p.id).collect();
        if !stale.is_empty() {
            call(&a, |tx| a.conn.reducers.admin_remove_posts_then(stale, tx))?;
            wait_for("the old post gone", || a.conn.db.trade_post().iter().all(|p| p.x != x || p.z != z))?;
        }
        posts.push(place_post(&a, id, x, y, z)?);
    }
    a.teleport(sx, sz)?;
    println!("placed 4 givers, 2 benches and a Saltreed Clump round the site, and 2 posts on the isle");

    // What can't be taken yet.
    let (road, bale, crossing) = (quest("cairnwick_road")?.wire, quest("first_bale")?.wire, quest("across_the_water")?.wire);
    refused(&a, "a quest above our level", "you need to be level 2", |tx| a.conn.reducers.accept_quest_then(odda, road, tx))?;
    refused(&a, "a quest from the wrong giver", "Brannoch Hale doesn't give that quest", |tx| a.conn.reducers.accept_quest_then(brannoch, crossing, tx))?;

    // Reed and Twine to half, by gathering and crafting, turned in early.
    accept(&a, odda, "reed_and_twine")?;
    while held(&a, pa.char_id, "saltreed") < 16 {
        call(&a, |tx| a.conn.reducers.gather_then(node, tx))?;
        crate::gather::wait_done(&a, reed, node)?;
        advance(&a, i64::from(reed.regrow_s) * 1_000_000)?;
    }
    reaches(&a, "reed_and_twine", 0, held(&a, pa.char_id, "saltreed"))?;
    grant(&a, "saltreed_twine", 4)?;
    craft(&a, joiner, "reedcloth", 1)?;
    reaches(&a, "reed_and_twine", 1, 1)?;
    turn_in_for(&a, odda, "reed_and_twine", 50)?;
    refused(&a, "a quest whose chain isn't done", "finish First Furrows first", |tx| a.conn.reducers.accept_quest_then(odda, bale, tx))?;

    // Skitters with B in the party, both counting, to half again.
    call(&a, |tx| a.conn.reducers.pick_skillset_then(1, tx))?;
    accept(&a, odda, "shore_pests")?;
    accept(&b, odda, "shore_pests")?;
    let nb = a.conn.db.character_public().iter().find(|c| c.char_id == b.presence().map_or(0, |p| p.char_id)).ok_or("no B")?.name;
    call(&a, |tx| a.conn.reducers.invite_to_party_then(nb.clone(), tx))?;
    wait_for("the invitation", || b.conn.db.my_party_invites().count() == 1)?;
    let inv = b.conn.db.my_party_invites().iter().next().ok_or("no invitation")?.scheduled_id;
    call(&b, |tx| b.conn.reducers.accept_party_invite_then(inv, tx))?;
    wait_for("the party", || a.conn.db.my_party().count() == 2)?;
    let mut summoned = Vec::new();
    let pests = quest("shore_pests")?.objectives[0].count;
    kill_n(&a, "shore_skitter", cap(pests) + 1, &mut summoned)?;
    reaches(&a, "shore_pests", 0, cap(pests))?;
    reaches(&b, "shore_pests", 0, cap(pests))?;
    println!("A killed {} Shore Skitters; A and B, partied and near, both count {}", cap(pests) + 1, cap(pests));
    turn_in_for(&a, odda, "shore_pests", 150)?;
    turn_in_for(&b, odda, "shore_pests", 150)?;
    call(&b, |tx| b.conn.reducers.leave_party_then(tx))?;

    // First Furrows: plant, ripen by the clock, harvest.
    accept(&a, odda, "first_furrows")?;
    let grain = sim::farm::crop_by_id("hearthgrain").ok_or("no hearthgrain")?;
    // Anything a failed run left in our row goes first.
    let stale: Vec<u64> = a.conn.db.crop().iter().filter(|c| c.owner != pa.char_id && (c.x - sx).hypot(c.z - sz) < 8.0).map(|c| c.id).collect();
    if !stale.is_empty() {
        call(&a, |tx| a.conn.reducers.admin_remove_crops_then(stale, tx))?;
    }
    grant(&a, "hearthgrain_seed", 8)?;
    a.teleport(sx, sz - 3.0)?;
    for k in 0..8 {
        plant(&a, pa.char_id, grain.seed, sx - 1.65 + 1.1 * (k % 4) as f32, sz - 3.6 + 1.2 * (k / 4) as f32)?;
    }
    wait_for("eight plantings", || a.conn.db.crop().iter().filter(|c| c.owner == pa.char_id).count() == 8)?;
    let last = a.conn.db.crop().iter().filter(|c| c.owner == pa.char_id).map(|c| c.ripe_at.to_micros_since_unix_epoch()).max().unwrap_or(0);
    advance(&a, last - crate::gather::game_now(&a) + 1_000_000)?;
    for c in a.conn.db.crop().iter().filter(|c| c.owner == pa.char_id).collect::<Vec<_>>() {
        call(&a, |tx| a.conn.reducers.harvest_then(c.id, tx))?;
    }
    a.teleport(sx, sz)?;
    let got = held(&a, pa.char_id, "hearthgrain");
    reaches(&a, "first_furrows", 0, got.min(cap(20)))?;
    turn_in_for(&a, odda, "first_furrows", (got.min(cap(20)) * 100 / 20).min(150))?;

    // A Bale for Market: craft one at the site, sell it on the isle, and get a skiff.
    accept(&a, odda, "first_bale")?;
    craft(&a, packwright, "grain_bale", 1)?;
    reaches(&a, "first_bale", 0, 1)?;
    let (ix, iz) = (ISLE_POSTS[0].1, ISLE_POSTS[0].2);
    a.teleport(ix + 1.0, iz)?;
    call(&a, |tx| a.conn.reducers.sell_pack_then(posts[0], tx))?;
    reaches(&a, "first_bale", 1, 1)?;
    a.teleport(sx, sz)?;
    turn_in_for(&a, odda, "first_bale", 100)?;
    wait_for("the skiff", || held(&a, pa.char_id, "driftpine_skiff") == 1)?;

    // Across the Water: a sale at the wrong post doesn't count; at Gullhaven Post it does.
    accept(&a, odda, "across_the_water")?;
    for (post, &(_, x, z)) in posts.iter().zip(&ISLE_POSTS) {
        a.teleport(sx, sz)?;
        call(&a, |tx| a.conn.reducers.dev_give_pack_then("grain_bale".into(), tx))?;
        wait_for("a pack", || a.conn.db.my_pack().count() == 1)?;
        a.teleport(x + 1.0, z)?;
        call(&a, |tx| a.conn.reducers.sell_pack_then(*post, tx))?;
        wait_for("the sale", || a.conn.db.my_pack().count() == 0)?;
    }
    reaches(&a, "across_the_water", 0, 1)?;
    a.teleport(sx, sz)?;
    refused(&a, "turning in to the wrong giver", "turn that quest in to Quill Ambry", |tx| a.conn.reducers.turn_in_quest_then(odda, crossing, tx))?;
    turn_in_for(&a, quill, "across_the_water", 100)?;

    // The rest at top level, where quest experience stops counting.
    call(&a, |tx| a.conn.reducers.dev_set_level_then(sim::combat::level_max(), tx))?;
    for set in [0, 2] {
        call(&a, |tx| a.conn.reducers.pick_skillset_then(set, tx))?;
    }
    accept(&a, odda, "cairnwick_road")?;
    turn_in(&a, brannoch, "cairnwick_road")?;
    for (giver_from, giver_to, id, kind) in [
        (brannoch, brannoch, "boar_trouble", "bristleback_boar"),
        (brannoch, brannoch, "wolves_in_the_wood", "greymane_wolf"),
        (odda, yeva, "fen_croakers", "fen_croaker"),
        (yeva, yeva, "hexers_tarn", "brine_hexer"),
    ] {
        accept(&a, giver_from, id)?;
        let n = quest(id)?.objectives[0].count;
        kill_n(&a, kind, n, &mut summoned)?;
        reaches(&a, id, 0, n)?;
        a.teleport(sx, sz)?;
        turn_in(&a, giver_to, id)?;
        println!("A: turned in {id} after {n} kills");
    }
    let all = sim::quest::QUESTS.len();
    wait_for("every quest turned in", || a.conn.db.my_quests().iter().filter(|e| e.turned_in_at.is_some()).count() == all)?;
    refused(&a, "a quest already done", "you've done that quest", |tx| a.conn.reducers.accept_quest_then(odda, road, tx))?;
    println!("A has turned in all {all} quests");

    // Clean up.
    for id in summoned {
        let _ = call(&a, |tx| a.conn.reducers.dev_remove_npc_then(id, tx));
    }
    let crops: Vec<u64> = a.conn.db.crop().iter().filter(|c| c.owner == pa.char_id).map(|c| c.id).collect();
    if !crops.is_empty() {
        call(&a, |tx| a.conn.reducers.admin_remove_crops_then(crops, tx))?;
    }
    call(&a, |tx| a.conn.reducers.admin_remove_givers_then(givers, tx))?;
    call(&a, |tx| a.conn.reducers.admin_remove_stations_then(vec![joiner, packwright], tx))?;
    call(&a, |tx| a.conn.reducers.admin_remove_nodes_then(vec![node], tx))?;
    call(&a, |tx| a.conn.reducers.admin_remove_posts_then(posts, tx))?;
    if own {
        let cells: Vec<u64> = OWN_CELLS.iter().map(|&(x, z)| cell_of(OPEN_WORLD, x, z)).collect();
        call(&a, |tx| a.conn.reducers.admin_remove_land_then(cells.clone(), tx))?;
        call(&a, |tx| a.conn.reducers.admin_remove_terrain_then(cells, tx))?;
    }
    for bot in [a, b] {
        bot.conn.disconnect().map_err(|e| e.to_string())?;
    }
    Ok(())
}
