use std::{
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// Cache namespace for one network runtime.
///
/// Resolver workers are process-shared, but answers are not shared across
/// partitions. A browser/network runtime creates one partition and retains it
/// for its lifetime, preventing another runtime from inheriting its DNS cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DnsCachePartition(NonZeroU64);

impl DnsCachePartition {
    /// Creates a process-unique DNS cache namespace.
    pub fn fresh() -> Self {
        static NEXT_PARTITION: AtomicU64 = AtomicU64::new(1);
        let partition = NEXT_PARTITION.fetch_add(1, Ordering::Relaxed);
        Self(
            NonZeroU64::new(partition)
                .expect("DNS cache partition identity must not wrap through zero"),
        )
    }
}

/// Exact host and port to resolve before a transport starts a connection.
///
/// The port participates in lookup coalescing and cache identity because
/// `ToSocketAddrs` resolves a socket endpoint rather than a bare hostname.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DnsTarget {
    host: Arc<str>,
    port: u16,
}

impl DnsTarget {
    pub fn new(host: impl Into<Arc<str>>, port: u16) -> Self {
        let host = host.into();
        assert!(!host.is_empty(), "DNS target host must not be empty");
        Self { host, port }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Exact identity shared by cache and in-flight lookup ownership.
///
/// The runtime partition is part of the key even when two runtimes resolve the
/// same endpoint, so neither cached answers nor in-flight subscribers can
/// cross their network ownership boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DnsLookupKey {
    pub(crate) partition: DnsCachePartition,
    pub(crate) target: DnsTarget,
}
