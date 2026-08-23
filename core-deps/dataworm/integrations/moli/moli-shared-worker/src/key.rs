use moli_storage_key::MoliStorageKey;

use crate::SharedWorkerSameSiteCookies;

/// SharedWorker matching key.
///
/// This follows Chromium's important shape: constructor storage key, resolved
/// script URL, worker name, and same-site-cookie mode identify the worker slot.
/// Script type and credentials are compatibility checks, not key components.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SharedWorkerKey {
    storage_key: MoliStorageKey,
    script_url: String,
    name: String,
    same_site_cookies: SharedWorkerSameSiteCookies,
}

impl SharedWorkerKey {
    /// Build a key from already-resolved constructor data.
    pub fn new(
        storage_key: MoliStorageKey,
        script_url: String,
        name: String,
        same_site_cookies: SharedWorkerSameSiteCookies,
    ) -> Self {
        Self {
            storage_key,
            script_url,
            name,
            same_site_cookies,
        }
    }

    /// Return the constructor-context storage key.
    pub fn storage_key(&self) -> &MoliStorageKey {
        &self.storage_key
    }

    /// Return the resolved worker script URL string.
    pub fn script_url(&self) -> &str {
        &self.script_url
    }

    /// Return the SharedWorker name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the same-site-cookie matching mode.
    pub fn same_site_cookies(&self) -> SharedWorkerSameSiteCookies {
        self.same_site_cookies
    }
}
