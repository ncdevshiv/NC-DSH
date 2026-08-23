//! Property-based tests for the TTL cache.
//!
//! Guarantees: values round-trip under their key while within capacity, and
//! the cache is strictly bounded — with more distinct keys than capacity,
//! the oldest insertions are evicted and the newest survive.

#![cfg(test)]

use std::time::Duration;

use proptest::prelude::*;

use crate::TtlCache;

proptest! {
    /// A value put under a key reads back unchanged while it is within
    /// capacity and TTL (duplicate keys keep the latest value).
    #[test]
    fn ttl_round_trip(
        entries in prop::collection::vec(
            ("[a-z]{1,8}", prop::collection::vec(any::<i64>(), 0..8)),
            1..20,
        ),
    ) {
        let cache = TtlCache::new(Duration::from_secs(60), 100);
        for (key, value) in &entries {
            let value = serde_json::to_value(value).unwrap();
            cache.set(key.clone(), value.clone());
        }
        // Duplicate keys keep the latest value — check the final state.
        let mut latest: std::collections::HashMap<&str, &Vec<i64>> = std::collections::HashMap::new();
        for (key, value) in &entries {
            latest.insert(key.as_str(), value);
        }
        for (key, value) in latest {
            let expected = serde_json::to_value(value).unwrap();
            prop_assert_eq!(&cache.get(key).expect("entry was just set"), &expected);
        }
    }

    /// The cache is bounded: with more distinct keys than capacity, the
    /// oldest insertions are evicted first and the most recent survive.
    #[test]
    fn ttl_evicts_oldest_beyond_capacity(
        pairs in prop::collection::vec((0usize..=1000, "[a-z]{1,6}"), 16..40),
    ) {
        let capacity = 8;
        let cache = TtlCache::new(Duration::from_secs(60), capacity);
        // Prefix with the index so every key is unique and insertion order
        // is fully determined.
        let keys: Vec<String> = pairs
            .iter()
            .map(|(i, s)| format!("{i}-{s}"))
            .collect();
        for (index, key) in keys.iter().enumerate() {
            cache.set(key.clone(), serde_json::json!(index));
        }
        prop_assert!(cache.len() <= capacity, "capacity exceeded: {}", cache.len());
        let total = keys.len();
        prop_assert_eq!(cache.len(), capacity.min(total));
        // The oldest key was evicted; the newest key is still present.
        prop_assert!(cache.get(&keys[0]).is_none(), "oldest key survived");
        prop_assert!(
            cache.get(&keys[total - 1]).is_some(),
            "newest key was evicted"
        );
    }
}
