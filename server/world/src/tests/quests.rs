//! Quests (docs/design/quests.md).

use super::land::{as_admin, give, held};
use super::*;
use crate::combat::fighter;
use crate::quests::{
    GiverPlacement, QuestLog, abandon_quest, accept_quest, admin_clear_givers, admin_put_givers, admin_remove_givers, my_quests, quest_giver, quest_log,
    turn_in_quest,
};
use sim::content::{BAG_SLOTS, QuestAct, item_by_id};
use sim::quest::{giver_by_id, quest_by_id};

const ADMIN: u128 = 99;

fn q(id: &str) -> u16 {
    quest_by_id(id).unwrap().wire
}

/// Place giver `id` at `(x, y, z)` as the admin; returns its id.
fn giver_at(t: &TestContext, id: &str, x: f32, y: f32, z: f32) -> u64 {
    as_admin(t);
    let p = GiverPlacement { kind: giver_by_id(id).unwrap().wire, x, y, z, yaw: 0 };
    call(t, ADMIN, |ctx| admin_put_givers(ctx, vec![p])).unwrap();
    read(t, |ctx| ctx.db.quest_giver().iter().map(|g| g.id).max().unwrap())
}

/// Odda and Brannoch beside the origin, where characters arrive.
fn givers(t: &TestContext) -> (u64, u64) {
    (giver_at(t, "odda_fernwright", 2.0, 0.0, 0.0), giver_at(t, "brannoch_hale", -2.0, 0.0, 0.0))
}

fn entry(t: &TestContext, char_id: u64, quest: u16) -> Option<QuestLog> {
    read(t, |ctx| ctx.db.quest_log().char_id().filter(char_id).find(|e| e.quest == quest))
}

fn done(t: &TestContext, char_id: u64, quest: u16) -> Vec<u32> {
    entry(t, char_id, quest).expect("on the quest").done
}

fn accept(t: &TestContext, player: u128, giver: u64, id: &str) -> Result<(), String> {
    call(t, player, |ctx| accept_quest(ctx, giver, q(id)))
}

fn turn_in(t: &TestContext, player: u128, giver: u64, id: &str) -> Result<(), String> {
    call(t, player, |ctx| turn_in_quest(ctx, giver, q(id)))
}

/// Count `n` of `act` on `target` for `char_id`, as the systems that report progress do.
fn count(t: &TestContext, char_id: u64, act: QuestAct, target: u16, post: Option<u8>, n: u32) {
    read(t, |ctx| crate::quests::count(ctx, char_id, act, target, post, n));
}

fn level_xp(t: &TestContext, char_id: u64) -> (u8, u32) {
    read(t, |ctx| ctx.db.fighter().char_id().find(char_id).map(|f| (f.level, f.xp)).unwrap())
}

fn set_level(t: &TestContext, char_id: u64, level: u8) {
    read(t, |ctx| {
        let f = ctx.db.fighter().char_id().find(char_id).unwrap();
        ctx.db.fighter().char_id().update(crate::combat::Fighter { level, xp: 0, ..f });
    });
}

/// Record `ids` as turned in by `char_id`, as if it had done them.
fn turned_in(t: &TestContext, char_id: u64, ids: &[&str]) {
    read(t, |ctx| {
        for id in ids {
            let n = quest_by_id(id).unwrap().objectives.len();
            ctx.db.quest_log().insert(QuestLog { id: 0, char_id, quest: q(id), done: vec![0; n], taken_at: crate::time::now(ctx), turned_in_at: Some(crate::time::now(ctx)) });
        }
    });
}

fn wire(id: &str) -> u16 {
    item_by_id(id).unwrap().wire
}

#[test]
fn admins_place_remove_and_clear_givers() {
    let t = world();
    arrive(&t, 1, "Mira");
    let p = GiverPlacement { kind: 0, x: 1.0, y: 0.0, z: 1.0, yaw: 0 };
    assert!(call(&t, 1, |ctx| admin_put_givers(ctx, vec![p.clone()])).is_err());
    as_admin(&t);
    assert!(call(&t, ADMIN, |ctx| admin_put_givers(ctx, vec![GiverPlacement { kind: 200, ..p.clone() }])).unwrap_err().contains("unknown kind"));
    assert!(call(&t, ADMIN, |ctx| admin_put_givers(ctx, vec![GiverPlacement { x: 1e6, ..p.clone() }])).unwrap_err().contains("off the map"));
    let a = giver_at(&t, "odda_fernwright", 1.0, 0.0, 1.0);
    let b = giver_at(&t, "quill_ambry", 100.0, 0.0, 1.0);
    let g = read(&t, |ctx| ctx.db.quest_giver().id().find(b).unwrap());
    assert_eq!((g.kind, g.cell), (3, sim::cell::cell_of(sim::cell::OPEN_WORLD, 100.0, 1.0)));
    assert!(call(&t, 1, |ctx| admin_remove_givers(ctx, vec![a])).is_err());
    call(&t, ADMIN, |ctx| admin_remove_givers(ctx, vec![a, 424_242])).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.quest_giver().count()), 1);
    call(&t, ADMIN, admin_clear_givers).unwrap();
    assert_eq!(read(&t, |ctx| ctx.db.quest_giver().count()), 0);
}

#[test]
fn taking_a_quest_needs_its_giver_near_the_level_and_the_quests_before() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let (odda, brannoch) = givers(&t);
    let far = giver_at(&t, "odda_fernwright", 30.0, 0.0, 0.0);
    assert_eq!(call(&t, 1, |ctx| accept_quest(ctx, odda, 999)).unwrap_err(), "no such quest");
    assert_eq!(accept(&t, 1, 424_242, "shore_pests").unwrap_err(), "nobody there");
    assert_eq!(accept(&t, 1, far, "shore_pests").unwrap_err(), "too far from Odda Fernwright");
    assert_eq!(accept(&t, 1, brannoch, "shore_pests").unwrap_err(), "Brannoch Hale doesn't give that quest");
    assert_eq!(accept(&t, 1, odda, "cairnwick_road").unwrap_err(), "you need to be level 2");
    set_level(&t, me.char_id, 2);
    assert_eq!(accept(&t, 1, odda, "cairnwick_road").unwrap_err(), "finish Pinchers on the Strand first");
    assert!(entry(&t, me.char_id, q("cairnwick_road")).is_none(), "refusals take nothing");
    accept(&t, 1, odda, "shore_pests").unwrap();
    let e = entry(&t, me.char_id, q("shore_pests")).unwrap();
    assert_eq!((e.done, e.turned_in_at), (vec![0], None));
    assert_eq!(accept(&t, 1, odda, "shore_pests").unwrap_err(), "you're already on that quest");
    // Only those in the world take quests.
    call(&t, 1, |ctx| {
        leave_world(ctx);
        Ok(())
    })
    .unwrap();
    assert_eq!(accept(&t, 1, odda, "first_furrows").unwrap_err(), "not in the world");
}

#[test]
fn objectives_count_their_own_acts_up_to_half_again() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let (odda, _) = givers(&t);
    accept(&t, 1, odda, "reed_and_twine").unwrap();
    accept(&t, 1, odda, "first_furrows").unwrap();
    let reed = wire("saltreed");
    count(&t, me.char_id, QuestAct::Gather, reed, None, 10);
    // Harvesting Saltreed, or gathering Reedcloth, isn't gathering Saltreed.
    count(&t, me.char_id, QuestAct::Harvest, reed, None, 10);
    count(&t, me.char_id, QuestAct::Gather, wire("reedcloth"), None, 10);
    assert_eq!(done(&t, me.char_id, q("reed_and_twine")), vec![10, 0]);
    assert_eq!(done(&t, me.char_id, q("first_furrows")), vec![0]);
    count(&t, me.char_id, QuestAct::Gather, reed, None, 100);
    count(&t, me.char_id, QuestAct::Craft, wire("reedcloth"), None, 1);
    assert_eq!(done(&t, me.char_id, q("reed_and_twine")), vec![48, 1]);
    count(&t, me.char_id, QuestAct::Craft, wire("reedcloth"), None, 9);
    assert_eq!(done(&t, me.char_id, q("reed_and_twine")), vec![48, 3]);
}

#[test]
fn gathering_and_crafting_count_and_half_done_turns_in_for_half() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let (odda, brannoch) = givers(&t);
    accept(&t, 1, odda, "reed_and_twine").unwrap();
    let node = super::gathering::put(&t, "saltreed_clump", 1.0, 1.0);
    call(&t, 1, |ctx| crate::gathering::gather(ctx, node)).unwrap();
    super::gathering::finish(&t, me.char_id);
    let got = held(&t, me.char_id, "saltreed");
    assert!(got >= 2);
    assert_eq!(done(&t, me.char_id, q("reed_and_twine")), vec![got, 0]);
    let bench = super::crafting::put(&t, "joiners_bench", 0.0, 1.0);
    give(&t, me.char_id, "saltreed_twine", 4);
    call(&t, 1, |ctx| crate::crafting::craft(ctx, bench, sim::craft::recipe_by_id("reedcloth").unwrap().wire, 1)).unwrap();
    super::crafting::finish_unit(&t, me.char_id);
    assert_eq!(done(&t, me.char_id, q("reed_and_twine")), vec![got, 1]);
    assert_eq!(turn_in(&t, 1, odda, "reed_and_twine").unwrap_err(), "that quest isn't far enough along");
    // Half of each: the reward in full and half the experience.
    count(&t, me.char_id, QuestAct::Gather, wire("saltreed"), None, 16 - got);
    assert_eq!(turn_in(&t, 1, brannoch, "reed_and_twine").unwrap_err(), "turn that quest in to Odda Fernwright");
    let before = level_xp(&t, me.char_id);
    let cloth = held(&t, me.char_id, "reedcloth");
    turn_in(&t, 1, odda, "reed_and_twine").unwrap();
    assert_eq!(level_xp(&t, me.char_id), sim::combat::gain_xp(before.0, before.1, 110));
    assert_eq!(held(&t, me.char_id, "reedcloth"), cloth + 2);
    assert!(entry(&t, me.char_id, q("reed_and_twine")).unwrap().turned_in_at.is_some());
    // Done is done: no more counting, no taking it again, no turning it in twice.
    count(&t, me.char_id, QuestAct::Gather, wire("saltreed"), None, 5);
    assert_eq!(done(&t, me.char_id, q("reed_and_twine")), vec![16, 1]);
    assert_eq!(accept(&t, 1, odda, "reed_and_twine").unwrap_err(), "you've done that quest");
    assert_eq!(turn_in(&t, 1, odda, "reed_and_twine").unwrap_err(), "you're not on that quest");
}

#[test]
fn harvesting_counts_the_crop() {
    let t = world();
    super::land::ground(&t);
    let me = super::farming::farmer(&t, 1, "Mira");
    super::land::set_position(&t, 1, 0.0, 0.0);
    let y = presence_of(&t, 1).y;
    let odda = giver_at(&t, "odda_fernwright", 2.0, y, 0.0);
    accept(&t, 1, odda, "first_furrows").unwrap();
    super::farming::sow(&t, 1, "hearthgrain_seed", 1.0, 0.0).unwrap();
    let c = super::farming::crops(&t)[0].clone();
    super::farming::ripen(&t, &c);
    call(&t, 1, |ctx| crate::farming::harvest(ctx, c.id)).unwrap();
    let got = held(&t, me.char_id, "hearthgrain");
    assert!(got >= 3);
    assert_eq!(done(&t, me.char_id, q("first_furrows")), vec![got]);
}

#[test]
fn packs_count_when_crafted_and_sold_at_the_right_post() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let (odda, _) = givers(&t);
    let quill = giver_at(&t, "quill_ambry", 0.0, 0.0, 2.0);
    set_level(&t, me.char_id, 3);
    assert_eq!(accept(&t, 1, odda, "first_bale").unwrap_err(), "finish First Furrows first");
    turned_in(&t, me.char_id, &["first_furrows", "reed_and_twine"]);
    accept(&t, 1, odda, "first_bale").unwrap();
    let bench = super::trade::bench(&t, 0.0, -1.0);
    super::trade::craft_bale(&t, 1, me.char_id, bench);
    assert_eq!(done(&t, me.char_id, q("first_bale")), vec![1, 0]);
    // Posts far enough from the bench to buy, and walks there and back.
    let redfen = super::trade::post(&t, "redfen_post", 1500.0, 0.0);
    let gullhaven = super::trade::post(&t, "gullhaven_post", -1500.0, 0.0);
    let sell_at = |post: u64, x: f32| {
        super::land::set_position(&t, 1, x, 1.0);
        let sold = call(&t, 1, |ctx| crate::trade::sell_pack(ctx, post));
        super::land::set_position(&t, 1, 0.0, 0.0);
        sold
    };
    sell_at(redfen, 1500.0).unwrap();
    assert_eq!(done(&t, me.char_id, q("first_bale")), vec![1, 1]);
    turn_in(&t, 1, odda, "first_bale").unwrap();
    assert_eq!(held(&t, me.char_id, "driftpine_skiff"), 1);

    // Across the water: only a sale at Gullhaven Post counts, and it's turned in to Quill.
    accept(&t, 1, odda, "across_the_water").unwrap();
    super::trade::give_pack(&t, 1, "grain_bale");
    sell_at(redfen, 1500.0).unwrap();
    assert_eq!(done(&t, me.char_id, q("across_the_water")), vec![0]);
    super::trade::give_pack(&t, 1, "grain_bale");
    sell_at(gullhaven, -1500.0).unwrap();
    assert_eq!(done(&t, me.char_id, q("across_the_water")), vec![1]);
    assert_eq!(turn_in(&t, 1, odda, "across_the_water").unwrap_err(), "turn that quest in to Quill Ambry");
    let before = level_xp(&t, me.char_id);
    turn_in(&t, 1, quill, "across_the_water").unwrap();
    assert_eq!(level_xp(&t, me.char_id), sim::combat::gain_xp(before.0, before.1, 800));
}

#[test]
fn an_errand_is_done_on_reaching_its_giver() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let (odda, brannoch) = givers(&t);
    set_level(&t, me.char_id, 2);
    turned_in(&t, me.char_id, &["shore_pests"]);
    accept(&t, 1, odda, "cairnwick_road").unwrap();
    assert_eq!(done(&t, me.char_id, q("cairnwick_road")), Vec::<u32>::new());
    assert_eq!(turn_in(&t, 1, odda, "cairnwick_road").unwrap_err(), "turn that quest in to Brannoch Hale");
    turn_in(&t, 1, brannoch, "cairnwick_road").unwrap();
    assert_eq!(level_xp(&t, me.char_id), sim::combat::gain_xp(2, 0, 100));
    // Brannoch's own quests open up.
    accept(&t, 1, brannoch, "boar_trouble").unwrap();
}

#[test]
fn a_reward_that_does_not_fit_turns_in_nothing() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let (odda, _) = givers(&t);
    accept(&t, 1, odda, "shore_pests").unwrap();
    count(&t, me.char_id, QuestAct::Kill, u16::from(sim::npc::npc_kind_by_id("shore_skitter").unwrap().wire), None, 6);
    give(&t, me.char_id, "reedcutter_knife", u32::from(BAG_SLOTS));
    let before = level_xp(&t, me.char_id);
    assert_eq!(turn_in(&t, 1, odda, "shore_pests").unwrap_err(), "your bag is too full for the reward");
    assert_eq!(level_xp(&t, me.char_id), before);
    assert!(entry(&t, me.char_id, q("shore_pests")).unwrap().turned_in_at.is_none());
}

#[test]
fn abandoning_drops_the_progress_and_it_may_be_taken_again() {
    let t = world();
    let me = arrive(&t, 1, "Mira");
    let (odda, _) = givers(&t);
    assert_eq!(call(&t, 1, |ctx| abandon_quest(ctx, q("shore_pests"))).unwrap_err(), "you're not on that quest");
    accept(&t, 1, odda, "shore_pests").unwrap();
    count(&t, me.char_id, QuestAct::Kill, 0, None, 4);
    call(&t, 1, |ctx| abandon_quest(ctx, q("shore_pests"))).unwrap();
    assert!(entry(&t, me.char_id, q("shore_pests")).is_none());
    accept(&t, 1, odda, "shore_pests").unwrap();
    assert_eq!(done(&t, me.char_id, q("shore_pests")), vec![0]);
    // A turned-in quest can't be abandoned.
    turned_in(&t, me.char_id, &["first_furrows"]);
    assert_eq!(call(&t, 1, |ctx| abandon_quest(ctx, q("first_furrows"))).unwrap_err(), "you're not on that quest");
}

#[test]
fn my_quests_shows_only_our_own() {
    let t = world();
    let a = arrive(&t, 1, "Mira");
    arrive(&t, 2, "Tobin");
    let (odda, _) = givers(&t);
    accept(&t, 1, odda, "shore_pests").unwrap();
    accept(&t, 1, odda, "first_furrows").unwrap();
    let seen = |player: u128| {
        let mut v: Vec<(u64, u16)> = my_quests(&t.view_context(auth(player))).into_iter().map(|e| (e.char_id, e.quest)).collect();
        v.sort();
        v
    };
    assert_eq!(seen(1), vec![(a.char_id, q("shore_pests")), (a.char_id, q("first_furrows"))]);
    assert!(seen(2).is_empty());
}

#[cfg(feature = "dev")]
mod fighting {
    use super::super::combat::{put, use_on};
    use super::super::npc::{set_hp, summon};
    use super::*;
    use crate::combat::pick_skillset;

    /// `player` kills a fresh `kind` 3 m north of where it stands.
    fn kill(t: &TestContext, player: u128, kind: &str) {
        let p = presence_of(t, player);
        let id = summon(t, player, kind, p.x, p.z - 3.0);
        // Clear of the last swing's cooldowns.
        advance_ms(t, 30_000);
        set_hp(t, id, 1);
        use_on(t, player, "threefold_cut", id).unwrap();
        assert!(super::super::combat::vitals_of(t, id).dead_at.is_some());
    }

    #[test]
    fn kills_count_for_the_party_near_the_kill_and_overachieving_pays_more() {
        let t = world();
        let a = arrive(&t, 1, "Mira");
        let b = arrive(&t, 2, "Tobin");
        let c = arrive(&t, 3, "Wren");
        let (odda, _) = givers(&t);
        super::super::party::group(&t, &[(1, &a, "Mira"), (2, &b, "Tobin")]);
        for p in [1, 2, 3] {
            accept(&t, p, odda, "shore_pests").unwrap();
        }
        call(&t, 1, |ctx| pick_skillset(ctx, 1)).unwrap();
        kill(&t, 1, "shore_skitter");
        assert_eq!(done(&t, a.char_id, q("shore_pests")), vec![1]);
        assert_eq!(done(&t, b.char_id, q("shore_pests")), vec![1], "a party member near the kill counts it");
        assert_eq!(done(&t, c.char_id, q("shore_pests")), vec![0], "others don't");
        // Another kind doesn't count, and a member far off misses out.
        kill(&t, 1, "bristleback_boar");
        put(&t, 2, 300.0, 0.0);
        kill(&t, 1, "shore_skitter");
        assert_eq!(done(&t, a.char_id, q("shore_pests")), vec![2]);
        assert_eq!(done(&t, b.char_id, q("shore_pests")), vec![1]);
        // Counting stops at half again; turning in there pays half again.
        for _ in 0..10 {
            kill(&t, 1, "shore_skitter");
        }
        assert_eq!(done(&t, a.char_id, q("shore_pests")), vec![9]);
        let before = level_xp(&t, a.char_id);
        turn_in(&t, 1, odda, "shore_pests").unwrap();
        assert_eq!(level_xp(&t, a.char_id), sim::combat::gain_xp(before.0, before.1, 180));
        assert_eq!(held(&t, a.char_id, "emberleaf_salve"), 2);
    }

    #[test]
    fn the_dead_cannot_take_or_turn_in() {
        let t = world();
        let me = arrive(&t, 1, "Mira");
        let (odda, _) = givers(&t);
        accept(&t, 1, odda, "shore_pests").unwrap();
        count(&t, me.char_id, QuestAct::Kill, 0, None, 6);
        call(&t, 1, |ctx| crate::combat::dev_set_vitals(ctx, 0, 0)).unwrap();
        assert!(turn_in(&t, 1, odda, "shore_pests").is_err());
        assert!(accept(&t, 1, odda, "first_furrows").is_err());
    }
}
