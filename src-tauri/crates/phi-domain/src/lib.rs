//! Domain entities, events and read models.
//!
//! The domain crate depends on nothing but `serde`; ports and plugins depend on
//! it, never the other way around.

pub mod agent;
pub mod events;
pub mod memory;
pub mod message;
pub mod view;
