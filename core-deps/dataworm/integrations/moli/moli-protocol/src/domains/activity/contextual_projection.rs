use url::Url;

use crate::conn::{BackgroundProtocolEvent, CdpConnection, CommandDispatchContext};
use crate::domains::network::{
    MainDocumentProgressBackgroundEventBarrier, MainDocumentProgressGate,
};

use super::output_payloads::ProtocolOutputPayloads;
use super::output_slot::ProtocolOutputSlot;

/// Exact protocol projection context for one already-frozen output batch.
///
/// The context selects only how a frozen fact is projected for a session or
/// command. It cannot inspect renderer state, select Page work, retry a
/// producer, or change the output families owned by the batch.
pub(in crate::domains) struct ProtocolOutputProjectionContext<'a> {
    pub(in crate::domains) session_id: Option<&'a str>,
    pub(in crate::domains) command: &'a mut CommandDispatchContext,
    pub(in crate::domains) subresource_frame_id: Option<&'a str>,
    pub(in crate::domains) subresource_document_url: Option<&'a Url>,
    pub(in crate::domains) subresource_timestamp: Option<f64>,
    pub(in crate::domains) subresource_network_request_id: Option<&'a str>,
}

/// Ordered contextual projection of protocol-local facts.
///
/// A plan is a static sequence from the same closed output enum used by
/// renderer publications, not a second trait-object registry. It is limited to
/// context-derived facts and never reads renderer readiness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ProtocolOutputProjectionPlan {
    pub(super) steps: &'static [ProtocolOutputSlot],
}

pub(super) const POST_SUBRESOURCE_FETCH_PROJECTION_STEPS: &[ProtocolOutputSlot] = &[
    ProtocolOutputSlot::NetworkBacklog,
    ProtocolOutputSlot::SubresourceFetchInterception,
];

pub(super) const POST_SUBRESOURCE_NETWORK_PROJECTION_STEPS: &[ProtocolOutputSlot] =
    &[ProtocolOutputSlot::NetworkBacklog];

impl ProtocolOutputProjectionPlan {
    pub(super) async fn project_into_protocol_events_async(
        self,
        conn: &mut CdpConnection,
        mut context: ProtocolOutputProjectionContext<'_>,
    ) {
        let mut prepared_payloads = self
            .steps
            .contains(&ProtocolOutputSlot::NetworkBacklog)
            .then(|| {
                ProtocolOutputPayloads::from_slot(
                    crate::domains::network::NetworkPreparedOutputSlot::from_outputs(
                        crate::domains::network::network_backlog_prepared_outputs(
                            conn,
                            context.session_id,
                            context.subresource_network_request_id.map(
                                crate::domains::network::NetworkBacklogPreferredRequestId::contextual_subresource,
                            ),
                        ),
                    ),
                )
            });
        for projection in self.steps.iter().copied() {
            projection
                .project_async(conn, &mut context, prepared_payloads.as_mut())
                .await;
        }
    }

    #[cfg(test)]
    pub(super) fn has_contextual_projection_payloads(&self) -> bool {
        self.steps.contains(&ProtocolOutputSlot::NetworkBacklog)
    }
}

impl<'a> ProtocolOutputProjectionContext<'a> {
    pub(in crate::domains) fn new(
        session_id: Option<&'a str>,
        command: &'a mut CommandDispatchContext,
    ) -> Self {
        Self {
            session_id,
            command,
            subresource_frame_id: None,
            subresource_document_url: None,
            subresource_timestamp: None,
            subresource_network_request_id: None,
        }
    }

    pub(super) fn with_subresource_filter(
        mut self,
        frame_id: &'a str,
        document_url: &'a Url,
        network_request_id: Option<&'a str>,
    ) -> Self {
        self.subresource_frame_id = Some(frame_id);
        self.subresource_document_url = Some(document_url);
        self.subresource_network_request_id = network_request_id;
        self
    }
}

/// Projection guard for main-document background events captured before the
/// response body becomes externally visible.
pub(super) struct MainDocumentBodyCompleteProjection<'a> {
    progress_gate: &'a mut MainDocumentProgressGate,
}

impl<'a> MainDocumentBodyCompleteProjection<'a> {
    pub(super) fn new(progress_gate: &'a mut MainDocumentProgressGate) -> Self {
        Self { progress_gate }
    }

    pub(super) fn project_background_events(self, out: &mut Vec<BackgroundProtocolEvent>) {
        MainDocumentProgressBackgroundEventBarrier::drain_until_body_finished_visible(
            out,
            self.progress_gate,
        );
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use crate::conn::CommandDispatchContext;

    use super::super::output_slot::ProtocolOutputSlot;
    use super::{
        POST_SUBRESOURCE_FETCH_PROJECTION_STEPS, POST_SUBRESOURCE_NETWORK_PROJECTION_STEPS,
        ProtocolOutputProjectionContext, ProtocolOutputProjectionPlan,
    };

    #[test]
    fn plans_use_closed_projection_families_with_exact_context() {
        assert_eq!(
            POST_SUBRESOURCE_FETCH_PROJECTION_STEPS,
            &[
                ProtocolOutputSlot::NetworkBacklog,
                ProtocolOutputSlot::SubresourceFetchInterception,
            ]
        );
        assert_eq!(
            POST_SUBRESOURCE_NETWORK_PROJECTION_STEPS,
            &[ProtocolOutputSlot::NetworkBacklog]
        );
        assert!(
            (ProtocolOutputProjectionPlan {
                steps: POST_SUBRESOURCE_NETWORK_PROJECTION_STEPS
            })
            .has_contextual_projection_payloads()
        );
        let document_url = Url::parse("https://example.test/page").unwrap();
        let mut command = CommandDispatchContext::default();
        let context = ProtocolOutputProjectionContext {
            session_id: Some("session-1"),
            command: &mut command,
            subresource_frame_id: Some("frame-1"),
            subresource_document_url: Some(&document_url),
            subresource_timestamp: Some(12.5),
            subresource_network_request_id: Some("REQ-1"),
        };

        assert_eq!(context.session_id, Some("session-1"));
        assert_eq!(context.subresource_frame_id, Some("frame-1"));
        assert_eq!(context.subresource_document_url, Some(&document_url));
        assert_eq!(context.subresource_network_request_id, Some("REQ-1"));
        assert_eq!(context.subresource_timestamp, Some(12.5));
    }
}
