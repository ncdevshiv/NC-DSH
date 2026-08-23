//! Chromium-style module script tree state machine.
//!
//! This crate owns only portable module-tree state. It does not fetch network
//! resources, compile V8 modules, mutate parser state, or advance document
//! lifecycle. The renderer supplies those operations through
//! [`ModuleScriptTreeHost`].

mod host;
mod job;
mod types;

pub use host::ModuleScriptTreeHost;
pub use job::ModuleScriptTreeJob;
pub use types::*;
