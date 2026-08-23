use std::collections::HashMap;

use moli_fetch::FetchCancelHandle;
use parking_lot::Mutex;

use super::registry::{ResourceLoadId, ResourceLoadKind};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DetachedKeepaliveLoadDiagnostics {
    pub(crate) active_load_count: usize,
}

struct DetachedKeepaliveLoad {
    _kind: ResourceLoadKind,
    cancel_handle: Option<FetchCancelHandle>,
}

/// Browser-runtime-owned tail of a keepalive request.
///
/// Entries here deliberately contain only network cancellation state and
/// diagnostics. A detached keepalive must not retain a Document, Worker, V8
/// context, promise resolver, or DOM completion route.
#[derive(Default)]
pub(crate) struct DetachedKeepaliveLoadRegistry {
    loads: Mutex<HashMap<ResourceLoadId, DetachedKeepaliveLoad>>,
}

impl DetachedKeepaliveLoadRegistry {
    pub(crate) fn insert(
        &self,
        id: ResourceLoadId,
        kind: ResourceLoadKind,
        cancel_handle: Option<FetchCancelHandle>,
    ) {
        let previous = self.loads.lock().insert(
            id,
            DetachedKeepaliveLoad {
                _kind: kind,
                cancel_handle,
            },
        );
        assert!(
            previous.is_none(),
            "detached keepalive load registered twice: {id:?}"
        );
    }

    pub(crate) fn attach_cancel_handle(
        &self,
        id: ResourceLoadId,
        cancel_handle: FetchCancelHandle,
    ) -> bool {
        let mut loads = self.loads.lock();
        let Some(load) = loads.get_mut(&id) else {
            return false;
        };
        load.cancel_handle = Some(cancel_handle);
        true
    }

    pub(crate) fn remove(&self, id: ResourceLoadId) -> bool {
        self.loads.lock().remove(&id).is_some()
    }

    pub(crate) fn diagnostics(&self) -> DetachedKeepaliveLoadDiagnostics {
        DetachedKeepaliveLoadDiagnostics {
            active_load_count: self.loads.lock().len(),
        }
    }
}
