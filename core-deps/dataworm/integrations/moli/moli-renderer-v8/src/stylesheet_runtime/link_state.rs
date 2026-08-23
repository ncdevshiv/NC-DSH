use std::sync::Arc;

use crate::frame_owner_model::MainDocumentStyleLoadEventBinding;

use super::load::StylesheetLinkClient;

#[derive(Debug)]
pub(in crate::document_runtime) struct LinkStyleState {
    active_load: Arc<StylesheetLinkClient>,
    resource_completion_successful: Option<bool>,
    import_completion_successful: Option<bool>,
    event_phase: LinkLoadEventPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkLoadEventPhase {
    WaitingForCompletion,
    Posted,
    Dispatched,
}

impl LinkStyleState {
    pub(super) fn new(
        active_load: Arc<StylesheetLinkClient>,
        import_completion_successful: Option<bool>,
    ) -> Self {
        Self {
            active_load,
            resource_completion_successful: None,
            import_completion_successful,
            event_phase: LinkLoadEventPhase::WaitingForCompletion,
        }
    }

    pub(super) fn active_load(&self) -> &Arc<StylesheetLinkClient> {
        &self.active_load
    }

    pub(super) fn is_pending(&self) -> bool {
        self.completion_successful().is_none()
    }

    pub(super) fn cancelable_load_event_binding(
        &self,
    ) -> Option<MainDocumentStyleLoadEventBinding> {
        (self.event_phase == LinkLoadEventPhase::WaitingForCompletion)
            .then(|| self.active_load.load_event_binding())
            .flatten()
    }

    pub(super) fn completion_successful(&self) -> Option<bool> {
        Some(self.resource_completion_successful? && self.import_completion_successful?)
    }

    pub(super) fn take_ready_event(&mut self) -> Option<(Arc<StylesheetLinkClient>, bool)> {
        if self.event_phase != LinkLoadEventPhase::WaitingForCompletion {
            return None;
        }
        let successful = self.completion_successful()?;
        self.event_phase = LinkLoadEventPhase::Posted;
        Some((Arc::clone(&self.active_load), successful))
    }

    pub(super) fn posted_event_load(&self) -> Option<&Arc<StylesheetLinkClient>> {
        (self.event_phase == LinkLoadEventPhase::Posted).then_some(&self.active_load)
    }

    pub(super) fn consume_posted_event(&mut self, load: &Arc<StylesheetLinkClient>) -> bool {
        if self.event_phase != LinkLoadEventPhase::Posted
            || !StylesheetLinkClient::ptr_eq(&self.active_load, load)
        {
            return false;
        }
        self.event_phase = LinkLoadEventPhase::Dispatched;
        true
    }

    pub(super) fn accept_resource_completion(
        &mut self,
        load: &Arc<StylesheetLinkClient>,
        successful: bool,
    ) -> bool {
        if !StylesheetLinkClient::ptr_eq(&self.active_load, load)
            || self.resource_completion_successful.is_some()
        {
            return false;
        }
        self.resource_completion_successful = Some(successful);
        true
    }

    pub(super) fn accept_import_completion(&mut self, successful: bool) -> bool {
        if self.import_completion_successful.is_some() {
            return false;
        }
        self.import_completion_successful = Some(successful);
        true
    }
}
