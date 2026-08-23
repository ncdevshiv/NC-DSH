use std::{fmt, sync::Arc};

use crate::{SharedWebStorageStore, new_shared_web_storage_store};

/// Web Storage state installed into one renderer page environment.
///
/// This type is intentionally independent from the network `ResourceRequestClient`.
/// `localStorage` belongs to the storage partition and `sessionStorage`
/// belongs to the browsing context; rebuilding the network backend must not
/// replace either store.
#[derive(Clone)]
pub struct RendererWebStorageHandles {
    local_storage: SharedWebStorageStore,
    session_storage: SharedWebStorageStore,
}

impl RendererWebStorageHandles {
    pub fn new(
        local_storage: SharedWebStorageStore,
        session_storage: SharedWebStorageStore,
    ) -> Self {
        Self {
            local_storage,
            session_storage,
        }
    }

    pub fn ephemeral() -> Self {
        Self::new(
            new_shared_web_storage_store(),
            new_shared_web_storage_store(),
        )
    }

    pub fn local_storage(&self) -> SharedWebStorageStore {
        self.local_storage.clone()
    }

    pub fn session_storage(&self) -> SharedWebStorageStore {
        self.session_storage.clone()
    }

    pub fn shares_local_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.local_storage, &other.local_storage)
    }

    pub fn shares_session_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.session_storage, &other.session_storage)
    }
}

impl Default for RendererWebStorageHandles {
    fn default() -> Self {
        Self::ephemeral()
    }
}

impl fmt::Debug for RendererWebStorageHandles {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RendererWebStorageHandles")
            .field(
                "local_storage_strong_count",
                &Arc::strong_count(&self.local_storage),
            )
            .field(
                "session_storage_strong_count",
                &Arc::strong_count(&self.session_storage),
            )
            .finish()
    }
}
