//! Crime (docs/design/crime.md): the kinds of evidence, what each is worth when reported, when it
//! fades, and when a character is Sought. The module decides who is a victim and writes the rows.

pub use crate::content::{CRIME, CrimeRules};

const MICROS_PER_S: i64 = 1_000_000;

/// What a piece of evidence is. The discriminant is the wire value in `evidence.kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Evidence {
    /// Left by a theft.
    Footprints = 0,
    /// Left by the first harm of a bout to a character that wasn't marked.
    Bloodstain = 1,
    /// Left by killing a character that wasn't marked.
    BloodPool = 2,
}

impl Evidence {
    pub fn from_u8(v: u8) -> Option<Self> {
        [Self::Footprints, Self::Bloodstain, Self::BloodPool].get(usize::from(v)).copied()
    }

    /// What players see.
    pub fn name(self) -> &'static str {
        match self {
            Self::Footprints => "Footprints",
            Self::Bloodstain => "Bloodstain",
            Self::BloodPool => "Blood Pool",
        }
    }

    /// The crime points (and Notoriety) a report of it adds.
    pub fn points(self) -> u32 {
        let p = match self {
            Self::Footprints => CRIME.footprint_points,
            Self::Bloodstain => CRIME.bloodstain_points,
            Self::BloodPool => CRIME.blood_pool_points,
        };
        p.unsigned_abs()
    }
}

/// When evidence made at `made_at` fades, both in µs since the Unix epoch.
pub fn fades_at(made_at: i64) -> i64 {
    made_at + i64::from(CRIME.evidence_s) * MICROS_PER_S
}

/// Whether evidence made at `made_at` has faded by `now`.
pub fn faded(made_at: i64, now: i64) -> bool {
    now >= fades_at(made_at)
}

/// When a mark put on at `now` runs out.
pub fn mark_until(now: i64) -> i64 {
    now + i64::from(CRIME.mark_s) * MICROS_PER_S
}

/// A character at `at` (its feet) can report evidence at `thing`: within reach horizontally, and
/// up or down.
pub fn in_reach(thing: [f32; 3], at: [f32; 3]) -> bool {
    let (dx, dz) = (at[0] - thing[0], at[2] - thing[2]);
    dx * dx + dz * dz <= CRIME.reach_m * CRIME.reach_m && (at[1] - thing[1]).abs() <= CRIME.reach_m
}

/// Whether a character with `crime_points` is Sought.
pub fn sought(crime_points: u32) -> bool {
    crime_points >= CRIME.sought_points.unsigned_abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_with_their_points() {
        for (wire, kind, points) in [(0, Evidence::Footprints, 3), (1, Evidence::Bloodstain, 1), (2, Evidence::BloodPool, 10)] {
            assert_eq!(Evidence::from_u8(wire), Some(kind));
            assert_eq!(kind as u8, wire);
            assert_eq!(kind.points(), points);
        }
        assert_eq!(Evidence::from_u8(3), None);
        assert_eq!(Evidence::BloodPool.name(), "Blood Pool");
    }

    #[test]
    fn evidence_fades_at_its_time() {
        let made = 1_000 * MICROS_PER_S;
        let at = made + 86_400 * MICROS_PER_S;
        assert_eq!(fades_at(made), at);
        assert!(!faded(made, at - 1));
        assert!(faded(made, at));
        assert_eq!(mark_until(made), made + 30 * MICROS_PER_S);
    }

    #[test]
    fn reach_is_horizontal_and_vertical() {
        let e = [10.0, 5.0, 10.0];
        assert!(in_reach(e, [13.9, 5.0, 10.0]));
        assert!(!in_reach(e, [12.9, 5.0, 12.9]));
        assert!(in_reach(e, [10.0, 9.0, 10.0]));
        assert!(!in_reach(e, [10.0, 9.1, 10.0]));
    }

    #[test]
    fn sought_from_the_threshold() {
        assert!(!sought(49));
        assert!(sought(50));
    }
}
