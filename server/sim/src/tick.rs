//! The world tick's rate and how far each tick steps the world (PLAN §4.4, docs/design/tick.md).

/// `world_tick` runs every 100 ms (10 Hz).
pub const TICK_MICROS: i64 = 100_000;

/// The most one tick may step the world. A longer gap (a stalled server, a restart, a dev clock
/// jump) is cut to this, so nothing driven by the tick jumps; the world runs slow for a moment
/// instead. Anything that must catch up exactly is computed from timestamps, not ticked.
pub const MAX_DT: f32 = 0.25;

/// Seconds to step the world, given the game time elapsed since the last tick.
pub fn step_dt(elapsed_micros: i64) -> f32 {
    (elapsed_micros.max(0) as f32 / 1_000_000.0).min(MAX_DT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_tick_steps_its_measured_time() {
        assert_eq!(step_dt(TICK_MICROS), 0.1);
        assert_eq!(step_dt(137_000), 0.137);
    }

    #[test]
    fn long_gaps_are_capped() {
        assert_eq!(step_dt(250_001), MAX_DT);
        assert_eq!(step_dt(3_600_000_000), MAX_DT);
    }

    #[test]
    fn time_never_runs_backwards() {
        assert_eq!(step_dt(0), 0.0);
        assert_eq!(step_dt(-5_000), 0.0);
    }
}
