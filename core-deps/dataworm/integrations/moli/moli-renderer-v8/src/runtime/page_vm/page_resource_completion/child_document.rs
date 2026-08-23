use anyhow::Result;

use crate::{
    page_resource_completion::{
        PageResourceCompletionOutputEffect, PageResourceCompletionTurnAction,
        RendererPageResourceCompletionOwner,
    },
    runtime::RendererOwnerResourceActivitySource,
    types::{ChildBlockingStylesheetLoadCompletion, ChildClassicScriptLoadCompletion},
};

use super::super::PageVm;

impl PageVm {
    pub(super) fn apply_child_classic_script_terminal(
        &mut self,
        source: RendererOwnerResourceActivitySource,
        owner: RendererPageResourceCompletionOwner,
        completion: ChildClassicScriptLoadCompletion,
    ) -> Result<PageResourceCompletionTurnAction> {
        let current_owner = self.current_page_resource_completion_owner(owner);
        if current_owner != Some(owner) {
            let output_effect = PageResourceCompletionOutputEffect::capture_if(
                self.vm_mut()
                    .record_historical_child_classic_script_network_result(&completion),
            );
            return Ok(PageResourceCompletionTurnAction::discarded_stale(
                source,
                owner,
                current_owner,
                output_effect,
            ));
        }

        self.vm_mut()
            .apply_child_classic_script_load_completion_from_page_turn(completion)?;
        Ok(PageResourceCompletionTurnAction::applied(
            source,
            owner,
            PageResourceCompletionOutputEffect::CaptureRequired,
        ))
    }

    pub(super) fn apply_child_blocking_stylesheet_terminal(
        &mut self,
        source: RendererOwnerResourceActivitySource,
        owner: RendererPageResourceCompletionOwner,
        completion: ChildBlockingStylesheetLoadCompletion,
    ) -> Result<PageResourceCompletionTurnAction> {
        let current_owner = self.current_page_resource_completion_owner(owner);
        if current_owner != Some(owner) {
            // The completed request remains a Page-observable Network fact,
            // but it must neither mutate nor advance activity for the current
            // replacement Document.
            self.vm_mut()
                .record_historical_child_blocking_stylesheet_network_results(&completion);
            return Ok(PageResourceCompletionTurnAction::discarded_stale(
                source,
                owner,
                current_owner,
                PageResourceCompletionOutputEffect::CaptureRequired,
            ));
        }

        self.vm_mut()
            .apply_child_blocking_stylesheet_load_completion_from_page_turn(completion)?;
        Ok(PageResourceCompletionTurnAction::applied(
            source,
            owner,
            PageResourceCompletionOutputEffect::CaptureRequired,
        ))
    }
}
