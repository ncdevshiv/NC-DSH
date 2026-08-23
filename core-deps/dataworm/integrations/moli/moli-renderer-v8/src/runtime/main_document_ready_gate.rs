use crate::document_script_scheduler::{
    DocumentOwnedScriptReadyAction, DocumentScriptReadyActionDispatchRoute,
    DocumentScriptReadyActionRoute, DocumentScriptReadyDispatch,
    DocumentScriptReadyDispatchOwnerMismatch, MainDocumentReadyActionRoute,
};
use crate::frame_owner_model::FrameDocumentTaskOwner;

use super::PageVm;

impl PageVm {
    pub(in crate::runtime) fn claim_main_document_ready_action<Action>(
        &self,
        action: Action,
        work_kind: &'static str,
    ) -> Option<Action>
    where
        Action: DocumentScriptReadyActionDispatchRoute<MainDocumentReadyActionRoute>
            + DocumentScriptReadyActionRoute<FrameDocumentTaskOwner>,
    {
        let queued_owner = action.payload_document_owner();
        let dispatch = match DocumentOwnedScriptReadyAction::new(queued_owner, action)
            .into_dispatch::<MainDocumentReadyActionRoute>()
        {
            Ok(dispatch) => dispatch,
            Err(mismatch) => {
                report_main_document_ready_owner_mismatch(mismatch, work_kind);
                return None;
            }
        };
        claim_main_document_ready_dispatch(
            dispatch,
            self.vm().current_main_document_task_owner(),
            work_kind,
        )
    }
}

pub(in crate::runtime) fn claim_main_document_ready_dispatch<Owner, Action>(
    dispatch: DocumentScriptReadyDispatch<Owner, Action, MainDocumentReadyActionRoute>,
    current_owner: Option<Owner>,
    work_kind: &'static str,
) -> Option<Action>
where
    Owner: Copy + std::fmt::Debug + PartialEq,
{
    let queued_owner = *dispatch.queued_owner();
    let route = *dispatch.route();
    if current_owner != Some(queued_owner) {
        tracing::debug!(
            work_kind,
            owner = ?queued_owner,
            script_node_id = ?route.script_node_id(),
            "dropping main document ready work for stale document task owner"
        );
        return None;
    }
    let (action, _route) = dispatch.into_action_and_route();
    Some(action)
}

pub(in crate::runtime) fn report_main_document_ready_owner_mismatch<Owner>(
    mismatch: DocumentScriptReadyDispatchOwnerMismatch<Owner, MainDocumentReadyActionRoute>,
    work_kind: &'static str,
) where
    Owner: std::fmt::Debug,
{
    tracing::debug!(
        work_kind,
        queued_owner = ?mismatch.queued_owner(),
        payload_owner = ?mismatch.payload_owner(),
        script_node_id = ?mismatch.route().script_node_id(),
        "dropping main document ready work queued under mismatched owner"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document_script_scheduler::{
            DocumentScriptReadyActionDispatchRoute, DocumentScriptReadyActionRoute,
        },
        dom::NodeId,
        frame_owner_model::{DocumentId, FrameSchedulerLaneId, LocalWindowId},
    };

    #[derive(Clone, Copy, Debug)]
    struct TestReadyAction {
        owner: FrameDocumentTaskOwner,
        route: MainDocumentReadyActionRoute,
    }

    impl DocumentScriptReadyActionRoute<FrameDocumentTaskOwner> for TestReadyAction {
        fn payload_document_owner(&self) -> FrameDocumentTaskOwner {
            self.owner
        }
    }

    impl DocumentScriptReadyActionDispatchRoute<MainDocumentReadyActionRoute> for TestReadyAction {
        fn dispatch_route(&self) -> MainDocumentReadyActionRoute {
            self.route
        }
    }

    #[test]
    fn main_ready_gate_rejects_replaced_document_owner_on_same_window() {
        let lane = FrameSchedulerLaneId(0);
        let window = LocalWindowId(0);
        let retired_owner = FrameDocumentTaskOwner::new(lane, window, DocumentId(0));
        let current_owner = FrameDocumentTaskOwner::new(lane, window, DocumentId(1));
        let action = TestReadyAction {
            owner: retired_owner,
            route: MainDocumentReadyActionRoute::new(NodeId::new(7)),
        };
        let dispatch = DocumentOwnedScriptReadyAction::new(retired_owner, action)
            .into_dispatch::<MainDocumentReadyActionRoute>()
            .expect("test ready action should match its queued owner");

        assert!(
            claim_main_document_ready_dispatch(
                dispatch,
                Some(current_owner),
                "test main ready action",
            )
            .is_none(),
            "replacement currentness must be decided by DocumentId, not browsing-context identity"
        );
    }
}
