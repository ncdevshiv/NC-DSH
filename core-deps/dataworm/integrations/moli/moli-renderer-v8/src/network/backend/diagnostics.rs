use super::SharedMemoryResourceCacheDiagnostics;
use crate::network::loads::DetachedKeepaliveLoadDiagnostics;

/// Snapshot of one browser-context scoped renderer resource runtime.
#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserResourceRuntimeDiagnostics {
    pub runtime_id: u64,
    pub memory_cache: SharedMemoryResourceCacheDiagnostics,
    pub(crate) detached_keepalive_loads: DetachedKeepaliveLoadDiagnostics,
}
