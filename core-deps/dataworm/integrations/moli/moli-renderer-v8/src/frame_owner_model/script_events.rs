use crate::document_runtime::DomHandle;

use super::records::FrameDocumentOwner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameDocumentScriptElementEvent {
    pub(crate) child_handle: DomHandle,
    pub(crate) owner: FrameDocumentOwner,
    pub(crate) script_handle: DomHandle,
    pub(crate) kind: FrameDocumentScriptElementEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameDocumentScriptElementEventKind {
    Load,
    Error,
}

impl FrameDocumentScriptElementEvent {
    pub(crate) fn load(
        child_handle: DomHandle,
        owner: FrameDocumentOwner,
        script_handle: DomHandle,
    ) -> Self {
        Self {
            child_handle,
            owner,
            script_handle,
            kind: FrameDocumentScriptElementEventKind::Load,
        }
    }

    pub(crate) fn error(
        child_handle: DomHandle,
        owner: FrameDocumentOwner,
        script_handle: DomHandle,
    ) -> Self {
        Self {
            child_handle,
            owner,
            script_handle,
            kind: FrameDocumentScriptElementEventKind::Error,
        }
    }
}

impl FrameDocumentScriptElementEventKind {
    pub(crate) fn event_type(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Error => "error",
        }
    }
}
