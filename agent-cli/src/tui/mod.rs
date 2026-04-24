pub mod icons;
pub mod layout;
pub mod shell;
pub mod suggestions;
pub mod theme;

// Legacy full-screen TUI building blocks are kept for tests/regression
// coverage, but the runtime now uses streaming non-fullscreen shell.
#[cfg(test)]
pub mod event_bridge;
#[cfg(test)]
pub mod history_search;
#[cfg(test)]
pub mod input;
#[cfg(test)]
pub mod input_buffer;
#[cfg(test)]
pub mod renderer;
pub mod transcript;
#[cfg(test)]
pub mod types;
#[cfg(test)]
pub mod view_model;
