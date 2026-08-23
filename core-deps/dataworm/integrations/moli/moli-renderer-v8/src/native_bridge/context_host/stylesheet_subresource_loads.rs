use super::JsContextHost;
use crate::{
    document_runtime::DomHandle,
    frame_owner_model::{FrameDocumentTaskOwner, StylesheetSubresourceLoadDelayBinding},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StylesheetSubresourceLoadDelaySettlement {
    settled: bool,
}

impl StylesheetSubresourceLoadDelaySettlement {
    pub(crate) fn settled(self) -> bool {
        self.settled
    }
}

impl JsContextHost {
    pub(crate) fn accept_current_main_stylesheet_subresource_load_delay(
        &mut self,
    ) -> Option<StylesheetSubresourceLoadDelayBinding> {
        let owner = self.current_main_document_task_owner()?;
        let binding = self
            .frame_owner_store
            .accept_current_main_stylesheet_subresource_load_delay(owner)?;
        tracing::debug!(
            ?owner,
            token = ?binding.load_delay_token(),
            "accepted main stylesheet subresource load delay"
        );
        Some(binding)
    }

    pub(crate) fn accept_current_child_stylesheet_subresource_load_delay(
        &mut self,
        child_handle: DomHandle,
        owner: FrameDocumentTaskOwner,
    ) -> Option<StylesheetSubresourceLoadDelayBinding> {
        let binding = self
            .frame_owner_store
            .accept_current_child_stylesheet_subresource_load_delay(child_handle, owner)?;
        tracing::debug!(
            ?child_handle,
            ?owner,
            token = ?binding.load_delay_token(),
            "accepted child stylesheet subresource load delay"
        );
        Some(binding)
    }

    pub(crate) fn stylesheet_subresource_load_delay_is_current(
        &self,
        binding: StylesheetSubresourceLoadDelayBinding,
    ) -> bool {
        self.frame_owner_store
            .stylesheet_subresource_load_delay_is_current(binding)
    }

    pub(crate) fn settle_stylesheet_subresource_load_delay(
        &mut self,
        binding: StylesheetSubresourceLoadDelayBinding,
    ) -> StylesheetSubresourceLoadDelaySettlement {
        let settled = self
            .frame_owner_store
            .settle_stylesheet_subresource_load_delay(binding);
        let queued_child_frame_task = if settled
            && binding.load_delay_token().is_some()
            && let Some(child_handle) = binding.child_handle()
        {
            self.queue_child_document_complete_lifecycle_if_ready_for_owner(
                child_handle,
                binding.owner(),
            )
        } else {
            false
        };
        tracing::debug!(
            owner = ?binding.owner(),
            child_handle = ?binding.child_handle(),
            token = ?binding.load_delay_token(),
            settled,
            queued_child_frame_task,
            "settled stylesheet subresource load delay"
        );
        StylesheetSubresourceLoadDelaySettlement { settled }
    }
}
