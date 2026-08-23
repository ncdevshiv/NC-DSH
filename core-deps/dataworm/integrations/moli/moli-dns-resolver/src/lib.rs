//! Bounded process-level DNS resolution for Moli network runtimes.
//!
//! This crate owns DNS lookup identity, positive caching, in-flight lookup
//! coalescing, and the bounded system-resolver worker pool. It deliberately
//! does not know about HTTP proxy policy, libcurl handles, browser pages, or
//! renderer lifecycle. Callers decide whether a request should use this
//! resolver and how a successful address list is installed on their transport.

mod identity;
mod lookup;
mod positive_cache;
mod service;
mod state;
mod worker_pool;

#[cfg(test)]
mod tests;

pub use identity::{DnsCachePartition, DnsTarget};
pub use lookup::DnsLookupResult;
pub use service::DnsResolverService;
