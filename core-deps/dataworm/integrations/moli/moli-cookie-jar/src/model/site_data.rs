//! Site-data summaries, clear targets, and before/after storage reports.

/// Cookie counts grouped by one registrable site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieSiteDataSummary {
    /// Registrable site key, such as `example.com`.
    pub name: String,
    /// Total unexpired cookies for this site.
    pub cookie_count: usize,
    /// Unexpired cookies with persistent expiration.
    pub persistent_cookie_count: usize,
    /// Unexpired session cookies.
    pub session_cookie_count: usize,
}

impl CookieSiteDataSummary {
    /// Builds a site summary and derives session count from the persistent count.
    pub fn new(name: String, cookie_count: usize, persistent_cookie_count: usize) -> Self {
        Self {
            name,
            cookie_count,
            persistent_cookie_count,
            session_cookie_count: cookie_count.saturating_sub(persistent_cookie_count),
        }
    }
}

/// Which cookie population should be summarized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieSiteDataScope {
    Live,
    Persistent,
}

/// Which cookie population should be removed by a clear operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieSiteDataClearScope {
    All,
    #[cfg(any(test, feature = "test-support"))]
    Persistent,
    #[cfg(any(test, feature = "test-support"))]
    Session,
}

/// Target set for a cookie-storage clear operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieStorageClearTarget {
    WholeStore,
    RegistrableSites(Vec<String>),
}

#[cfg(any(test, feature = "test-support"))]
/// Test-support command shape for site-data operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieSiteDataOperation {
    Clear {
        target: CookieStorageClearTarget,
        scope: CookieSiteDataClearScope,
    },
}

/// Result of clearing cookies for site-data APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieSiteDataClearReport {
    /// Requested clear scope.
    pub scope: CookieSiteDataClearScope,
    /// Normalized site keys requested by the caller.
    pub requested_sites: Vec<String>,
    /// Number of cookies actually removed.
    pub removed_cookie_count: usize,
    /// Snapshot before the clear operation.
    pub replaced_state: CookieStorageStateSnapshot,
    /// Snapshot after the clear operation.
    pub resulting_state: CookieStorageStateSnapshot,
    /// Difference between `replaced_state` and `resulting_state`.
    pub state_diff: CookieStorageStateDiff,
}

#[cfg(any(test, feature = "test-support"))]
/// Dry-run result for clearing cookies for site-data APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieSiteDataClearPreviewReport {
    /// Requested clear scope.
    pub scope: CookieSiteDataClearScope,
    /// Normalized site keys requested by the caller.
    pub requested_sites: Vec<String>,
    /// Number of cookies that would be removed.
    pub would_remove_cookie_count: usize,
    /// Snapshot before the simulated clear operation.
    pub replaced_state: CookieStorageStateSnapshot,
    /// Snapshot after the simulated clear operation.
    pub resulting_state: CookieStorageStateSnapshot,
    /// Difference between `replaced_state` and `resulting_state`.
    pub state_diff: CookieStorageStateDiff,
}

/// Result of clearing the whole store or selected registrable sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieStorageClearReport {
    /// Store-wide or site-targeted clear target.
    pub target: CookieStorageClearTarget,
    /// Requested clear scope.
    pub scope: CookieSiteDataClearScope,
    /// Number of cookies actually removed.
    pub removed_cookie_count: usize,
    /// Snapshot before the clear operation.
    pub replaced_state: CookieStorageStateSnapshot,
    /// Snapshot after the clear operation.
    pub resulting_state: CookieStorageStateSnapshot,
    /// Difference between `replaced_state` and `resulting_state`.
    pub state_diff: CookieStorageStateDiff,
}

#[cfg(any(test, feature = "test-support"))]
/// Dry-run result for clearing the whole store or selected registrable sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieStorageClearPreviewReport {
    /// Store-wide or site-targeted clear target.
    pub target: CookieStorageClearTarget,
    /// Requested clear scope.
    pub scope: CookieSiteDataClearScope,
    /// Number of cookies that would be removed.
    pub would_remove_cookie_count: usize,
    /// Snapshot before the simulated clear operation.
    pub replaced_state: CookieStorageStateSnapshot,
    /// Snapshot after the simulated clear operation.
    pub resulting_state: CookieStorageStateSnapshot,
    /// Difference between `replaced_state` and `resulting_state`.
    pub state_diff: CookieStorageStateDiff,
}

#[cfg(any(test, feature = "test-support"))]
/// Dry-run report for a site-data operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieSiteDataOperationPreviewReport {
    Clear(CookieStorageClearPreviewReport),
}

#[cfg(any(test, feature = "test-support"))]
impl CookieSiteDataOperationPreviewReport {
    /// Returns the simulated post-operation storage snapshot.
    pub fn resulting_state(&self) -> &CookieStorageStateSnapshot {
        match self {
            Self::Clear(report) => &report.resulting_state,
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
/// Applied report for a site-data operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieSiteDataOperationReport {
    Clear(CookieStorageClearReport),
}

/// Before/after summary for one registrable site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieSiteDataChange {
    /// Registrable site key.
    pub name: String,
    /// Site summary before the operation, or `None` if absent.
    pub before: Option<CookieSiteDataSummary>,
    /// Site summary after the operation, or `None` if absent.
    pub after: Option<CookieSiteDataSummary>,
}

/// Difference between two cookie storage snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CookieStorageStateDiff {
    /// Changes in the live unexpired cookie view.
    pub live_site_changes: Vec<CookieSiteDataChange>,
    /// Changes in the persistent-only cookie view.
    pub persistent_site_changes: Vec<CookieSiteDataChange>,
}

/// Cookie storage snapshot split into live and persistent site summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieStorageStateSnapshot {
    /// Store generation at snapshot time, when available.
    pub store_generation: Option<u64>,
    /// Total unexpired cookies in the live view.
    pub live_cookie_count: usize,
    /// Live view grouped by registrable site.
    pub live_site_data: Vec<CookieSiteDataSummary>,
    /// Total unexpired persistent cookies.
    pub persistent_cookie_count: usize,
    /// Persistent-only view grouped by registrable site.
    pub persistent_site_data: Vec<CookieSiteDataSummary>,
}
