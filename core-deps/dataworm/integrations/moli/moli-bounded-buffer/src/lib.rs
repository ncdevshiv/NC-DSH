//! An insertion-ordered key/value buffer with explicit byte budgets.
//!
//! Callers provide each value's logical byte charge. Evicted and rejected
//! values are returned so the owning subsystem can preserve its own metadata
//! or terminal state without this crate depending on that policy.

mod buffer;

pub use buffer::{BoundedByteBuffer, ByteLimits, InsertOutcome};

#[cfg(test)]
mod tests;
