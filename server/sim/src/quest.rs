//! Quests (docs/design/quests.md): how far along a quest is, when it may be turned in and what it
//! pays. The module keeps the quest log and counts what characters do.

pub use crate::content::{GIVERS, GiverData, ObjectiveData, QUEST, QUESTS, QuestAct, QuestData, QuestRules, RewardData, giver, giver_by_id, quest, quest_by_id};

/// The most quests a character may have open at once.
pub fn log_size() -> usize {
    QUEST.log.unsigned_abs() as usize
}

/// How far an objective of `need` may count: past `need` to `over_pct` of it when `need` is 2 or
/// more, so there's something to overachieve; just `need` for a single act.
pub fn cap(need: u32) -> u32 {
    if need < 2 { need } else { (u64::from(need) * u64::from(QUEST.over_pct.unsigned_abs()) / 100).max(u64::from(need)) as u32 }
}

/// An objective's count after `add` more, up to its cap.
pub fn tally(done: u32, need: u32, add: u32) -> u32 {
    done.saturating_add(add).min(cap(need))
}

/// How far along a quest is, %: its least advanced objective's share, or 100 for an errand.
pub fn score(objectives: &[ObjectiveData], done: &[u32]) -> u32 {
    objectives
        .iter()
        .enumerate()
        .map(|(k, o)| {
            let d = u64::from(done.get(k).copied().unwrap_or(0).min(cap(o.count)));
            (d * 100 / u64::from(o.count.max(1))) as u32
        })
        .min()
        .unwrap_or(100)
}

/// Whether a quest at `score` may be turned in.
pub fn ready(score: u32) -> bool {
    score >= QUEST.early_pct.unsigned_abs()
}

/// The experience a quest worth `xp` pays at `score`: its share, early or over.
pub fn xp_for(xp: u32, score: u32) -> u32 {
    u32::try_from(u64::from(xp) * u64::from(score) / 100).unwrap_or(u32::MAX)
}

/// How a quest stands, for the log and the tracker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Not far enough to turn in.
    Going,
    /// May be turned in for less than full.
    Early,
    /// Every objective met.
    Done,
    /// Past done, still counting.
    Extra,
    /// As far as it counts.
    Over,
}

/// How a quest with `objectives` stands at `done`.
pub fn standing(objectives: &[ObjectiveData], done: &[u32]) -> Standing {
    let s = score(objectives, done);
    let capped = objectives.iter().enumerate().all(|(k, o)| done.get(k).copied().unwrap_or(0) >= cap(o.count));
    if !ready(s) {
        Standing::Going
    } else if s < 100 {
        Standing::Early
    } else if capped && objectives.iter().any(|o| cap(o.count) > o.count) {
        Standing::Over
    } else if objectives.iter().enumerate().any(|(k, o)| done.get(k).copied().unwrap_or(0) > o.count) {
        Standing::Extra
    } else {
        Standing::Done
    }
}

/// How much of a `count` of `act` on `target` at post kind `post` adds to objective `o`: all of it
/// if it matches, nothing if not.
pub fn counts(o: &ObjectiveData, act: QuestAct, target: u16, post: Option<u8>) -> bool {
    o.act == act && o.target == target && (o.post.is_none() || o.post == post)
}

/// Whether a character at `level` who has turned in `done` (quest wires) may take `q`.
pub fn may_take(q: &QuestData, level: u8, done: &[u16]) -> Result<(), Refusal> {
    if level < q.level {
        return Err(Refusal::Level(q.level));
    }
    match q.after.iter().find(|a| !done.contains(a)) {
        Some(&a) => Err(Refusal::After(a)),
        None => Ok(()),
    }
}

/// Why a quest can't be taken yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The level it needs.
    Level(u8),
    /// A quest, by wire, to turn in first.
    After(u16),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kill(count: u32) -> ObjectiveData {
        ObjectiveData { act: QuestAct::Kill, target: 0, count, post: None }
    }

    #[test]
    fn objectives_count_on_to_half_again() {
        assert_eq!(QUEST.over_pct, 150);
        assert_eq!(cap(1), 1);
        assert_eq!(cap(2), 3);
        assert_eq!(cap(6), 9);
        assert_eq!(cap(7), 10);
        assert_eq!(tally(5, 6, 1), 6);
        assert_eq!(tally(8, 6, 5), 9);
        assert_eq!(tally(0, 1, 3), 1);
        assert_eq!(tally(u32::MAX, 6, 1), 9);
    }

    #[test]
    fn the_least_advanced_objective_sets_the_score() {
        let os = [kill(6), kill(2)];
        assert_eq!(score(&os, &[0, 0]), 0);
        assert_eq!(score(&os, &[6, 1]), 50);
        assert_eq!(score(&os, &[9, 3]), 150);
        assert_eq!(score(&os, &[9, 2]), 100);
        // A short list counts as nothing done; too much counts only to the cap.
        assert_eq!(score(&os, &[6]), 0);
        assert_eq!(score(&os, &[600, 600]), 150);
        assert_eq!(score(&[], &[]), 100);
        assert_eq!(score(&[kill(3)], &[1]), 33);
    }

    #[test]
    fn half_done_may_be_turned_in_for_its_share() {
        assert!(!ready(49));
        assert!(ready(50));
        assert_eq!(xp_for(120, 50), 60);
        assert_eq!(xp_for(120, 150), 180);
        assert_eq!(xp_for(101, 33), 33);
        assert_eq!(xp_for(u32::MAX, 300), u32::MAX);
    }

    #[test]
    fn standing_follows_the_score() {
        let os = [kill(6)];
        assert_eq!(standing(&os, &[2]), Standing::Going);
        assert_eq!(standing(&os, &[3]), Standing::Early);
        assert_eq!(standing(&os, &[6]), Standing::Done);
        assert_eq!(standing(&os, &[7]), Standing::Extra);
        assert_eq!(standing(&os, &[9]), Standing::Over);
        // A single act is done and no more; an errand is done at once.
        assert_eq!(standing(&[kill(1)], &[1]), Standing::Done);
        assert_eq!(standing(&[], &[]), Standing::Done);
        // One objective at its cap and another merely done: still extra.
        assert_eq!(standing(&[kill(6), kill(2)], &[9, 2]), Standing::Extra);
    }

    #[test]
    fn objectives_count_their_own_act_target_and_post() {
        let any = ObjectiveData { act: QuestAct::Sell, target: 500, count: 1, post: None };
        let isle = ObjectiveData { post: Some(2), ..any };
        assert!(counts(&any, QuestAct::Sell, 500, Some(0)));
        assert!(!counts(&any, QuestAct::Craft, 500, None));
        assert!(!counts(&any, QuestAct::Sell, 501, Some(0)));
        assert!(counts(&isle, QuestAct::Sell, 500, Some(2)));
        assert!(!counts(&isle, QuestAct::Sell, 500, Some(0)));
    }

    #[test]
    fn taking_needs_the_level_and_the_quests_before() {
        let q = quest_by_id("hexers_tarn").unwrap();
        let before = quest_by_id("fen_croakers").unwrap().wire;
        assert_eq!(may_take(q, 4, &[before]), Err(Refusal::Level(5)));
        assert_eq!(may_take(q, 5, &[]), Err(Refusal::After(before)));
        assert_eq!(may_take(q, 5, &[before]), Ok(()));
    }

    #[test]
    fn the_coast_quests_hang_together() {
        assert_eq!(log_size(), 50);
        assert_eq!(QUESTS.len(), 10);
        for q in &QUESTS {
            assert!(giver(q.giver).is_some() && giver(q.to).is_some(), "{}", q.id);
            // Every quest's experience at its best stays under a level's worth at its level.
            assert!(xp_for(q.xp, score(q.objectives, &q.objectives.iter().map(|o| cap(o.count)).collect::<Vec<_>>())) < 2000, "{}", q.id);
        }
    }
}
