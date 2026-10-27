//! Pure game rules shared by the server module and (through generated test vectors) the client.
//! No SpacetimeDB dependency, so everything here is plain unit-testable Rust.

pub mod cell;
pub mod content;
pub mod craft;
pub mod gather;
pub mod labor;
pub mod movement;
pub mod ship;
pub mod terrain;
pub mod tick;
#[cfg(test)]
mod vectors;
