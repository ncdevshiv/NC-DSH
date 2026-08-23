use std::{
    collections::{BTreeMap, HashMap},
    net::IpAddr,
    sync::Arc,
    time::Instant,
};

use crate::identity::DnsLookupKey;

struct DnsCachedAnswer {
    addresses: Arc<[IpAddr]>,
    expires_at: Instant,
}

/// Partitioned positive-answer cache with expiration-ordered retirement.
///
/// `answers` remains the exact-key authority. `expirations` is only an index
/// used to find entries whose TTL has elapsed; an older scheduled expiry may
/// therefore coexist with a refreshed answer. Retirement always rechecks the
/// authoritative answer's current deadline before removing it.
#[derive(Default)]
pub(crate) struct DnsPositiveCache {
    answers: HashMap<DnsLookupKey, DnsCachedAnswer>,
    expirations: BTreeMap<Instant, Vec<DnsLookupKey>>,
}

impl DnsPositiveCache {
    /// Returns one unexpired exact-key answer after retiring only due buckets.
    ///
    /// The work is proportional to entries that have expired since the last
    /// admission rather than to every live answer across every partition.
    pub(crate) fn get(&mut self, key: &DnsLookupKey, now: Instant) -> Option<Arc<[IpAddr]>> {
        self.retire_expired(now);
        let answer = self.answers.get(key)?;
        if answer.expires_at <= now {
            // This exact-key guard keeps stale answers impossible even if an
            // indexing defect ever leaves an entry outside its expiry bucket.
            self.answers.remove(key);
            return None;
        }
        Some(Arc::clone(&answer.addresses))
    }

    pub(crate) fn insert(
        &mut self,
        key: DnsLookupKey,
        addresses: Arc<[IpAddr]>,
        expires_at: Instant,
    ) {
        self.answers.insert(
            key.clone(),
            DnsCachedAnswer {
                addresses,
                expires_at,
            },
        );
        self.expirations.entry(expires_at).or_default().push(key);
    }

    fn retire_expired(&mut self, now: Instant) {
        while let Some(first) = self.expirations.first_entry() {
            if *first.key() > now {
                break;
            }
            let keys = first.remove();
            for key in keys {
                if self
                    .answers
                    .get(&key)
                    .is_some_and(|answer| answer.expires_at <= now)
                {
                    self.answers.remove(&key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{DnsCachePartition, DnsTarget};

    fn key(host: &str) -> DnsLookupKey {
        DnsLookupKey {
            partition: DnsCachePartition::fresh(),
            target: DnsTarget::new(host, 443),
        }
    }

    fn addresses(last_octet: u8) -> Arc<[IpAddr]> {
        Arc::from([IpAddr::from([127, 0, 0, last_octet])])
    }

    #[test]
    fn lookup_retires_only_due_expiry_buckets() {
        let now = Instant::now();
        let expired_key = key("expired.test");
        let live_key = key("live.test");
        let live_addresses = addresses(2);
        let mut cache = DnsPositiveCache::default();
        cache.insert(
            expired_key.clone(),
            addresses(1),
            now + Duration::from_secs(1),
        );
        cache.insert(
            live_key.clone(),
            Arc::clone(&live_addresses),
            now + Duration::from_secs(10),
        );

        assert_eq!(
            cache.get(&live_key, now + Duration::from_secs(2)),
            Some(live_addresses)
        );
        assert!(!cache.answers.contains_key(&expired_key));
        assert!(cache.answers.contains_key(&live_key));
        assert_eq!(cache.expirations.len(), 1);
    }

    #[test]
    fn stale_expiry_bucket_does_not_remove_refreshed_answer() {
        let now = Instant::now();
        let key = key("refreshed.test");
        let refreshed_addresses = addresses(2);
        let mut cache = DnsPositiveCache::default();
        cache.insert(key.clone(), addresses(1), now + Duration::from_secs(1));
        cache.insert(
            key.clone(),
            Arc::clone(&refreshed_addresses),
            now + Duration::from_secs(10),
        );

        assert_eq!(
            cache.get(&key, now + Duration::from_secs(2)),
            Some(refreshed_addresses)
        );
        assert_eq!(cache.answers.len(), 1);
        assert_eq!(cache.expirations.len(), 1);
    }
}
