pub mod events;

#[path = "../../agent-core/src/events.rs"]
mod legacy_events;

pub use events::*;
pub use legacy_events::*;

pub fn version() -> u32 {
    1
}
