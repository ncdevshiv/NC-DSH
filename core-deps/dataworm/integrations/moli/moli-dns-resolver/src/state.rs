use std::{collections::HashMap, sync::Arc, time::Instant};

use crate::{DnsLookupResult, identity::DnsLookupKey, positive_cache::DnsPositiveCache};

pub(crate) type DnsCompletion = Box<dyn FnOnce(DnsLookupResult) + Send + 'static>;

#[derive(Default)]
pub(crate) struct DnsResolverState {
    cache: DnsPositiveCache,
    in_flight: HashMap<DnsLookupKey, Vec<DnsCompletion>>,
}

pub(crate) enum DnsLookupAdmission {
    CompleteCached {
        addresses: Arc<[std::net::IpAddr]>,
        completion: DnsCompletion,
    },
    Coalesced,
    Start(DnsLookupKey),
}

impl DnsResolverState {
    /// Atomically observes cache state and installs the completion residence.
    ///
    /// A `Start` result means this call owns the only worker command for the
    /// key. A `Coalesced` result means another lookup owns that command and the
    /// completion remains resident here until its terminal result arrives.
    pub(crate) fn admit(
        &mut self,
        key: DnsLookupKey,
        completion: DnsCompletion,
        now: Instant,
    ) -> DnsLookupAdmission {
        if let Some(addresses) = self.cache.get(&key, now) {
            return DnsLookupAdmission::CompleteCached {
                addresses,
                completion,
            };
        }
        if let Some(completions) = self.in_flight.get_mut(&key) {
            completions.push(completion);
            return DnsLookupAdmission::Coalesced;
        }
        self.in_flight.insert(key.clone(), vec![completion]);
        DnsLookupAdmission::Start(key)
    }

    /// Consumes all completions for an exact lookup and caches only success.
    ///
    /// Callers must invoke returned completions after releasing the state lock;
    /// completion code may immediately submit another lookup.
    pub(crate) fn finish(
        &mut self,
        key: DnsLookupKey,
        result: &DnsLookupResult,
        expires_at: Instant,
    ) -> Vec<DnsCompletion> {
        let completions = self.in_flight.remove(&key).unwrap_or_default();
        if let Ok(addresses) = result {
            self.cache.insert(key, Arc::clone(addresses), expires_at);
        }
        completions
    }
}
