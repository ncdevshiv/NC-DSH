//! JSON serialization with a hard output-size limit.
//!
//! The limit is enforced while `serde_json` writes, so callers never allocate
//! the complete encoded value before discovering that it is too large.

mod error;
mod serializer;

pub use error::BoundedJsonError;
pub use serializer::{json_string_between_with_limit, to_string_with_limit};

#[cfg(test)]
mod tests;
