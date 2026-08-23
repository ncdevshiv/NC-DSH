use moli_cookie_jar::{
    BrowserCookieStorageAccessStatus, StoredCookieBrowserContextValueSource,
    StoredCookieFacadeStatus, StoredCookieSetRejectionReason,
};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCookieCapabilitySnapshot {
    pub cookies_enabled_preference: bool,
    pub facade_status: StoredCookieFacadeStatus,
    pub view_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentCookieCacheStatus {
    NoEntry,
    Reusable,
    PolicyBlocked,
    StoreUnavailable,
    UrlMismatch,
    StoreGenerationMismatch,
    FacadeGenerationMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCookieCacheSnapshot {
    pub status: DocumentCookieCacheStatus,
    pub cached_url: Option<Url>,
    pub cached_store_generation: Option<u64>,
    pub current_store_generation: Option<u64>,
    pub cached_facade_generation: Option<u64>,
    pub current_facade_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentCookieFirstOperation {
    Get,
    Set,
    CookiesEnabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentCookieCacheLookupResult {
    CacheMissFirstAccess,
    CacheHitAfterGet,
    CacheHitAfterSet,
    CacheMissAfterGet,
    CacheMissAfterSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCookieFacadeTelemetrySnapshot {
    pub first_operation: Option<DocumentCookieFirstOperation>,
    pub last_cache_lookup_result: Option<DocumentCookieCacheLookupResult>,
    pub last_operation_was_set: Option<bool>,
    pub cache_hits: u64,
    pub store_reads: u64,
    pub blocked_reads: u64,
    pub unavailable_reads: u64,
    pub applied_writes: u64,
    pub rejected_writes: u64,
    pub facade_blocked_writes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentCookieBackendConnectionState {
    Attached,
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentCookieGetFreshnessStatus {
    PolicyBlocked,
    NeedsBackendReconnect,
    Reusable,
    NoEntry,
    UrlMismatch,
    StoreGenerationMismatch,
    FacadeGenerationMismatch,
    NeedsRevalidationAfterDocumentWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentCookieSetReadinessStatus {
    PolicyBlocked,
    NeedsBackendReconnect,
    ReadyWillInvalidateCache,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCookieWriteCapabilitySnapshot {
    pub write_enabled: bool,
    pub primary_rejection_reason: Option<StoredCookieSetRejectionReason>,
    pub blocked_reasons: Vec<StoredCookieSetRejectionReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCookieBrowserContextSnapshot {
    pub site_for_cookies_url: Option<Url>,
    pub site_for_cookies_source: StoredCookieBrowserContextValueSource,
    pub top_frame_origin_url: Option<Url>,
    pub top_frame_origin_source: StoredCookieBrowserContextValueSource,
    pub storage_access_status: BrowserCookieStorageAccessStatus,
    pub storage_access_source: StoredCookieBrowserContextValueSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCookieOwnerSnapshot {
    pub capability: DocumentCookieCapabilitySnapshot,
    pub cache: DocumentCookieCacheSnapshot,
    pub browser_context: DocumentCookieBrowserContextSnapshot,
    pub write_capability: DocumentCookieWriteCapabilitySnapshot,
    pub backend_connection_state: DocumentCookieBackendConnectionState,
    pub cookie_get_freshness_status: DocumentCookieGetFreshnessStatus,
    pub cookie_set_readiness_status: DocumentCookieSetReadinessStatus,
    pub cookie_get_would_need_backend_access: bool,
    pub cookie_get_would_need_backend_reconnect: bool,
    pub cookie_get_would_hit_cache: bool,
    pub cookie_get_would_revalidate_after_write: bool,
    pub first_cookie_request: Option<DocumentCookieFirstOperation>,
    pub telemetry: DocumentCookieFacadeTelemetrySnapshot,
}

impl From<crate::renderer::DocumentCookieCapabilitySnapshot> for DocumentCookieCapabilitySnapshot {
    fn from(value: crate::renderer::DocumentCookieCapabilitySnapshot) -> Self {
        Self {
            cookies_enabled_preference: value.cookies_enabled_preference,
            facade_status: value.facade_status,
            view_generation: value.view_generation,
        }
    }
}

impl From<crate::renderer::DocumentCookieCacheStatus> for DocumentCookieCacheStatus {
    fn from(value: crate::renderer::DocumentCookieCacheStatus) -> Self {
        match value {
            crate::renderer::DocumentCookieCacheStatus::NoEntry => Self::NoEntry,
            crate::renderer::DocumentCookieCacheStatus::Reusable => Self::Reusable,
            crate::renderer::DocumentCookieCacheStatus::PolicyBlocked => Self::PolicyBlocked,
            crate::renderer::DocumentCookieCacheStatus::StoreUnavailable => Self::StoreUnavailable,
            crate::renderer::DocumentCookieCacheStatus::UrlMismatch => Self::UrlMismatch,
            crate::renderer::DocumentCookieCacheStatus::StoreGenerationMismatch => {
                Self::StoreGenerationMismatch
            }
            crate::renderer::DocumentCookieCacheStatus::FacadeGenerationMismatch => {
                Self::FacadeGenerationMismatch
            }
        }
    }
}

impl From<crate::renderer::DocumentCookieCacheSnapshot> for DocumentCookieCacheSnapshot {
    fn from(value: crate::renderer::DocumentCookieCacheSnapshot) -> Self {
        Self {
            status: value.status.into(),
            cached_url: value.cached_url,
            cached_store_generation: value.cached_store_generation,
            current_store_generation: value.current_store_generation,
            cached_facade_generation: value.cached_facade_generation,
            current_facade_generation: value.current_facade_generation,
        }
    }
}

impl From<crate::renderer::DocumentCookieFirstOperation> for DocumentCookieFirstOperation {
    fn from(value: crate::renderer::DocumentCookieFirstOperation) -> Self {
        match value {
            crate::renderer::DocumentCookieFirstOperation::Get => Self::Get,
            crate::renderer::DocumentCookieFirstOperation::Set => Self::Set,
            crate::renderer::DocumentCookieFirstOperation::CookiesEnabled => Self::CookiesEnabled,
        }
    }
}

impl From<crate::renderer::DocumentCookieCacheLookupResult> for DocumentCookieCacheLookupResult {
    fn from(value: crate::renderer::DocumentCookieCacheLookupResult) -> Self {
        match value {
            crate::renderer::DocumentCookieCacheLookupResult::CacheMissFirstAccess => {
                Self::CacheMissFirstAccess
            }
            crate::renderer::DocumentCookieCacheLookupResult::CacheHitAfterGet => {
                Self::CacheHitAfterGet
            }
            crate::renderer::DocumentCookieCacheLookupResult::CacheHitAfterSet => {
                Self::CacheHitAfterSet
            }
            crate::renderer::DocumentCookieCacheLookupResult::CacheMissAfterGet => {
                Self::CacheMissAfterGet
            }
            crate::renderer::DocumentCookieCacheLookupResult::CacheMissAfterSet => {
                Self::CacheMissAfterSet
            }
        }
    }
}

impl From<crate::renderer::DocumentCookieFacadeTelemetrySnapshot>
    for DocumentCookieFacadeTelemetrySnapshot
{
    fn from(value: crate::renderer::DocumentCookieFacadeTelemetrySnapshot) -> Self {
        Self {
            first_operation: value.first_operation.map(Into::into),
            last_cache_lookup_result: value.last_cache_lookup_result.map(Into::into),
            last_operation_was_set: value.last_operation_was_set,
            cache_hits: value.cache_hits,
            store_reads: value.store_reads,
            blocked_reads: value.blocked_reads,
            unavailable_reads: value.unavailable_reads,
            applied_writes: value.applied_writes,
            rejected_writes: value.rejected_writes,
            facade_blocked_writes: value.facade_blocked_writes,
        }
    }
}

impl From<crate::renderer::DocumentCookieBackendConnectionState>
    for DocumentCookieBackendConnectionState
{
    fn from(value: crate::renderer::DocumentCookieBackendConnectionState) -> Self {
        match value {
            crate::renderer::DocumentCookieBackendConnectionState::Attached => Self::Attached,
            crate::renderer::DocumentCookieBackendConnectionState::Disconnected => {
                Self::Disconnected
            }
        }
    }
}

impl From<crate::renderer::DocumentCookieGetFreshnessStatus> for DocumentCookieGetFreshnessStatus {
    fn from(value: crate::renderer::DocumentCookieGetFreshnessStatus) -> Self {
        match value {
            crate::renderer::DocumentCookieGetFreshnessStatus::PolicyBlocked => Self::PolicyBlocked,
            crate::renderer::DocumentCookieGetFreshnessStatus::NeedsBackendReconnect => {
                Self::NeedsBackendReconnect
            }
            crate::renderer::DocumentCookieGetFreshnessStatus::Reusable => Self::Reusable,
            crate::renderer::DocumentCookieGetFreshnessStatus::NoEntry => Self::NoEntry,
            crate::renderer::DocumentCookieGetFreshnessStatus::UrlMismatch => Self::UrlMismatch,
            crate::renderer::DocumentCookieGetFreshnessStatus::StoreGenerationMismatch => {
                Self::StoreGenerationMismatch
            }
            crate::renderer::DocumentCookieGetFreshnessStatus::FacadeGenerationMismatch => {
                Self::FacadeGenerationMismatch
            }
            crate::renderer::DocumentCookieGetFreshnessStatus::NeedsRevalidationAfterDocumentWrite => {
                Self::NeedsRevalidationAfterDocumentWrite
            }
        }
    }
}

impl From<crate::renderer::DocumentCookieSetReadinessStatus> for DocumentCookieSetReadinessStatus {
    fn from(value: crate::renderer::DocumentCookieSetReadinessStatus) -> Self {
        match value {
            crate::renderer::DocumentCookieSetReadinessStatus::PolicyBlocked => Self::PolicyBlocked,
            crate::renderer::DocumentCookieSetReadinessStatus::NeedsBackendReconnect => {
                Self::NeedsBackendReconnect
            }
            crate::renderer::DocumentCookieSetReadinessStatus::ReadyWillInvalidateCache => {
                Self::ReadyWillInvalidateCache
            }
        }
    }
}

impl From<crate::renderer::DocumentCookieWriteCapabilitySnapshot>
    for DocumentCookieWriteCapabilitySnapshot
{
    fn from(value: crate::renderer::DocumentCookieWriteCapabilitySnapshot) -> Self {
        Self {
            write_enabled: value.write_enabled,
            primary_rejection_reason: value.primary_rejection_reason,
            blocked_reasons: value.blocked_reasons,
        }
    }
}

impl From<crate::renderer::DocumentCookieBrowserContextSnapshot>
    for DocumentCookieBrowserContextSnapshot
{
    fn from(value: crate::renderer::DocumentCookieBrowserContextSnapshot) -> Self {
        Self {
            site_for_cookies_url: value.site_for_cookies_url,
            site_for_cookies_source: value.site_for_cookies_source,
            top_frame_origin_url: value.top_frame_origin_url,
            top_frame_origin_source: value.top_frame_origin_source,
            storage_access_status: value.storage_access_status,
            storage_access_source: value.storage_access_source,
        }
    }
}

impl From<crate::renderer::DocumentCookieOwnerSnapshot> for DocumentCookieOwnerSnapshot {
    fn from(value: crate::renderer::DocumentCookieOwnerSnapshot) -> Self {
        Self {
            capability: value.capability.into(),
            cache: value.cache.into(),
            browser_context: value.browser_context.into(),
            write_capability: value.write_capability.into(),
            backend_connection_state: value.backend_connection_state.into(),
            cookie_get_freshness_status: value.cookie_get_freshness_status.into(),
            cookie_set_readiness_status: value.cookie_set_readiness_status.into(),
            cookie_get_would_need_backend_access: value.cookie_get_would_need_backend_access,
            cookie_get_would_need_backend_reconnect: value.cookie_get_would_need_backend_reconnect,
            cookie_get_would_hit_cache: value.cookie_get_would_hit_cache,
            cookie_get_would_revalidate_after_write: value.cookie_get_would_revalidate_after_write,
            first_cookie_request: value.first_cookie_request.map(Into::into),
            telemetry: value.telemetry.into(),
        }
    }
}
