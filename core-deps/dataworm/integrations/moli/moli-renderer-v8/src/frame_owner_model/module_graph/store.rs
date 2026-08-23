use std::collections::BTreeMap;

use crate::frame_owner_model::LocalWindowId;

use super::document::ChildDocumentModulatorEntry;

#[derive(Default)]
pub(crate) struct ChildDocumentModulatorStore {
    pub(super) documents: BTreeMap<LocalWindowId, ChildDocumentModulatorEntry>,
}
