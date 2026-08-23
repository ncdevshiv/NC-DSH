use std::sync::Arc;

use crate::store::OpfsInner;

/// Exclusive backend lock retained after a namespace mutation commits.
///
/// Storage-owner adapters wrap this lease in their completion payload so a
/// competing ticket still observes the move/remove lock until the callback
/// boundary. Standalone callers release it directly by dropping it; neither
/// path runs filesystem IO.
#[derive(Debug)]
#[must_use = "dropping the mutation lease releases its OPFS path locks"]
pub struct OpfsMutationLease {
    inner: Arc<OpfsInner>,
    owner_id: u64,
}

impl OpfsMutationLease {
    pub(crate) fn new(inner: Arc<OpfsInner>, owner_id: u64) -> Self {
        Self { inner, owner_id }
    }
}

impl Drop for OpfsMutationLease {
    fn drop(&mut self) {
        self.inner.release_lock_owner(self.owner_id);
    }
}
