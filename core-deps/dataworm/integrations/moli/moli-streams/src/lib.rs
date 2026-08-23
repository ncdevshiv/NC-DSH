//! Runtime-independent state transitions for Web Streams.
//!
//! This crate owns pure descriptor metadata and transition planning. Runtime
//! adapters remain responsible for traced buffers, JavaScript callbacks,
//! promise settlement, and storage identity. Immediate plans are committed
//! without running author JavaScript between snapshot and apply; continuations
//! that cross JavaScript must decode fresh state, while destructive payload
//! operations validate their adapter-owned queue generation before commit.

#![forbid(unsafe_code)]

pub mod numeric;
pub mod pipe;
pub mod queue;
pub mod readable;
pub mod strategy;
pub mod tee;
pub mod transfer;
pub mod transform;
pub mod writable;
