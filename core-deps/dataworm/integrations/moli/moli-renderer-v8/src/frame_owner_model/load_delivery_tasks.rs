use crate::document_runtime::DomHandle;

use super::records::{FrameDocumentLoadDispatchFinish, FrameDocumentTaskOwner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Exact-owner delivery action emitted after the document complete transition.
pub(crate) struct FrameDocumentLoadDeliveryTask {
    pub(crate) child_handle: DomHandle,
    pub(crate) owner: FrameDocumentTaskOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameDocumentLoadDeliveryAdmissionId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameDocumentLoadDeliveryAdmission {
    task: FrameDocumentLoadDeliveryTask,
    admission_id: FrameDocumentLoadDeliveryAdmissionId,
}

impl FrameDocumentLoadDeliveryAdmission {
    pub(super) const fn new(
        task: FrameDocumentLoadDeliveryTask,
        admission_id: FrameDocumentLoadDeliveryAdmissionId,
    ) -> Self {
        Self { task, admission_id }
    }

    pub(crate) const fn task(self) -> FrameDocumentLoadDeliveryTask {
        self.task
    }

    pub(crate) const fn admission_id(self) -> FrameDocumentLoadDeliveryAdmissionId {
        self.admission_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameDocumentLoadDeliveryPhase {
    WindowLoad,
    OwnerElementLoad,
    PageShow,
    FrameFinish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameDocumentLoadDeliveryAction {
    task: FrameDocumentLoadDeliveryTask,
    phase: FrameDocumentLoadDeliveryPhase,
}

impl FrameDocumentLoadDeliveryAction {
    pub(super) fn new(
        task: FrameDocumentLoadDeliveryTask,
        phase: FrameDocumentLoadDeliveryPhase,
    ) -> Self {
        Self { task, phase }
    }

    pub(crate) fn task(self) -> FrameDocumentLoadDeliveryTask {
        self.task
    }

    pub(crate) fn child_handle(self) -> DomHandle {
        self.task.child_handle
    }

    pub(crate) fn owner(self) -> FrameDocumentTaskOwner {
        self.task.owner
    }

    pub(crate) fn phase(self) -> FrameDocumentLoadDeliveryPhase {
        self.phase
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FrameDocumentLoadDeliveryProgress {
    Continue(FrameDocumentLoadDeliveryTask),
    /// The preceding observable phase completed, but a descendant load
    /// accepted during that phase must finish before frame delivery resumes.
    AwaitingDescendantCompletion(FrameDocumentLoadDeliveryTask),
    Finished(FrameDocumentLoadDispatchFinish),
}
