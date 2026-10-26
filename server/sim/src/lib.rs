//! Pure game rules shared by the server module and (through generated test vectors) the client.
//! No SpacetimeDB dependency, so everything here is plain unit-testable Rust.

pub mod cell;
pub mod movement;
