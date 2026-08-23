use crate::document_runtime::DomHandle;

use super::records::{
    FrameDocumentNavigationLoadBinding, FrameDocumentTaskOwner, FrameLaneTaskOwner, FrameRequestId,
};

/// Exact PageVm-local owner of one in-flight child-document navigation fetch.
///
/// The fetch belongs to the Document that initiated the navigation, while a
/// successful terminal creates a replacement Document. Consequently this
/// target deliberately contains no destination realm. The stable Page queue
/// adds the root renderer Document token before the terminal crosses a PageVm
/// replacement boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChildDocumentNavigationFetchTarget {
    child_handle: DomHandle,
    load_id: u64,
    navigation_load: FrameDocumentNavigationLoadBinding,
    request_id: FrameRequestId,
}

impl ChildDocumentNavigationFetchTarget {
    pub(crate) fn new(
        child_handle: DomHandle,
        load_id: u64,
        navigation_load: FrameDocumentNavigationLoadBinding,
        request_id: FrameRequestId,
    ) -> Self {
        Self {
            child_handle,
            load_id,
            navigation_load,
            request_id,
        }
    }

    pub(crate) fn child_handle(self) -> DomHandle {
        self.child_handle
    }

    pub(crate) fn load_id(self) -> u64 {
        self.load_id
    }

    pub(crate) fn navigation_load(self) -> FrameDocumentNavigationLoadBinding {
        self.navigation_load
    }

    pub(crate) fn task_owner(self) -> FrameDocumentTaskOwner {
        self.navigation_load.owner()
    }

    pub(crate) fn request_id(self) -> FrameRequestId {
        self.request_id
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        load_id: u64,
        request_id: FrameRequestId,
    ) -> Self {
        Self::new(
            child_handle,
            load_id,
            FrameDocumentNavigationLoadBinding::new(
                task_owner,
                super::records::FrameNavigationId(load_id),
                None,
            ),
            request_id,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameLaneNavigationCommitTask {
    pub(crate) child_handle: DomHandle,
    pub(crate) owner: FrameLaneTaskOwner,
    pub(crate) navigation_load: FrameDocumentNavigationLoadBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameNavigationCommitReservationResult {
    Reserved,
    AlreadyReserved,
    NotCurrent,
}

impl FrameLaneNavigationCommitTask {
    #[cfg(test)]
    pub(crate) fn for_test(
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        navigation_id: u64,
    ) -> Self {
        Self {
            child_handle,
            owner: FrameLaneTaskOwner::new(task_owner.scheduler_lane_id),
            navigation_load: FrameDocumentNavigationLoadBinding::new(
                task_owner,
                super::records::FrameNavigationId(navigation_id),
                None,
            ),
        }
    }
}
