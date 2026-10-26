//! Combat's public event table (PLAN §5.2). Skills come in M1; for now it carries cannon impacts and
//! the dev stand-in swing.

/// One hit or miss, broadcast to the cell it happened in.
#[spacetimedb::table(accessor = combat_event, public, event)]
pub struct CombatEvent {
    pub cell: u64,
    pub source: u64,
    /// The entity hit, or 0 for a miss.
    pub target: u64,
    pub skill_id: u32,
    pub amount: i32,
    pub flags: u16,
}

/// `combat_event.skill_id` until skills come from content.
pub mod skill {
    /// The dev stand-in swing (`dev_swing`).
    #[cfg(feature = "dev")]
    pub const SWING: u32 = 1;
    pub const CANNON: u32 = 2;
}

/// `combat_event.flags` bits.
pub mod flags {
    pub const HIT: u16 = 1;
}
