use std::sync::Arc;

use super::{
    RendererOutputCursor, RendererOutputFenceLeaseId, RendererOutputTransportMessage,
    RendererOutputTransportSender,
};

struct RendererOutputFenceLease {
    id: RendererOutputFenceLeaseId,
    cursor: RendererOutputCursor,
    transport: RendererOutputTransportSender,
}

impl Drop for RendererOutputFenceLease {
    fn drop(&mut self) {
        // A closed receiver is the terminal protocol boundary. Releasing the
        // local lease must remain non-panicking during command cancellation or
        // renderer teardown.
        let _ = self
            .transport
            .send(RendererOutputTransportMessage::CursorLeaseReleased {
                stream: self.cursor.stream(),
                lease_id: self.id,
            });
    }
}

/// One ordered renderer cursor together with its protocol-retention leases.
///
/// Composite commands still wait for only the latest cursor in one stream.
/// The internal leases are not additional ordering predecessors: they keep a
/// closed-stream tombstone alive until every clone or merged command result
/// that can query the cursor has been consumed or dropped.
#[derive(Clone)]
pub struct RendererOutputFence(Arc<RendererOutputFenceState>);

struct RendererOutputFenceState {
    cursor: RendererOutputCursor,
    leases: Vec<Arc<RendererOutputFenceLease>>,
}

impl RendererOutputFence {
    pub(crate) fn declare(
        cursor: RendererOutputCursor,
        transport: Option<RendererOutputTransportSender>,
    ) -> Self {
        let leases = transport
            .and_then(|transport| {
                let id = RendererOutputFenceLeaseId::allocate();
                transport
                    .send(RendererOutputTransportMessage::CursorLeaseDeclared {
                        cursor,
                        lease_id: id,
                    })
                    .ok()
                    .map(|()| {
                        vec![Arc::new(RendererOutputFenceLease {
                            id,
                            cursor,
                            transport,
                        })]
                    })
            })
            .unwrap_or_default();
        Self(Arc::new(RendererOutputFenceState { cursor, leases }))
    }

    pub fn cursor(&self) -> RendererOutputCursor {
        self.0.cursor
    }

    /// Joins independently owned lifetime leases while preserving a single
    /// FIFO ordering position.
    pub fn latest_in_same_stream(self, other: Self) -> Self {
        let cursor = self.cursor().latest_in_same_stream(other.cursor());
        if Arc::ptr_eq(&self.0, &other.0) {
            return self;
        }
        let mut leases = self.0.leases.clone();
        leases.extend(other.0.leases.iter().cloned());
        Self(Arc::new(RendererOutputFenceState { cursor, leases }))
    }

    /// Moves this fence into one optional same-stream tail.
    ///
    /// Ordering still collapses to one latest cursor; all lifetime leases are
    /// retained so an intermediate command result cannot release a closed
    /// stream while the composite result can still query it.
    pub fn merge_into_same_stream_tail(self, tail: &mut Option<Self>) {
        let merged = match tail.take() {
            Some(current) => current.latest_in_same_stream(self),
            None => self,
        };
        *tail = Some(merged);
    }

    #[doc(hidden)]
    pub fn new_for_test(cursor: RendererOutputCursor) -> Self {
        Self(Arc::new(RendererOutputFenceState {
            cursor,
            leases: Vec::new(),
        }))
    }

    fn lease_ids(&self) -> impl Iterator<Item = RendererOutputFenceLeaseId> + '_ {
        self.0.leases.iter().map(|lease| lease.id)
    }
}

impl std::fmt::Debug for RendererOutputFence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RendererOutputFence")
            .field("cursor", &self.cursor())
            .field("lease_ids", &self.lease_ids().collect::<Vec<_>>())
            .finish()
    }
}

impl PartialEq for RendererOutputFence {
    fn eq(&self, other: &Self) -> bool {
        self.cursor() == other.cursor() && self.lease_ids().eq(other.lease_ids())
    }
}

impl Eq for RendererOutputFence {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageId;
    use crate::runtime::RendererOutputStreamIdentity;

    #[test]
    fn last_fence_clone_releases_one_transport_lease() {
        let stream =
            RendererOutputStreamIdentity::new_page_for_protocol_test(PageId::new_for_testing(7));
        let cursor = RendererOutputCursor::new_for_test(stream, 3);
        let (transport, mut receiver) = crate::runtime::renderer_output_transport_channel();

        let fence = RendererOutputFence::declare(cursor, Some(transport));
        let lease_id = match receiver.try_recv().expect("cursor lease declaration") {
            RendererOutputTransportMessage::CursorLeaseDeclared {
                cursor: actual,
                lease_id,
            } => {
                assert_eq!(actual, cursor);
                lease_id
            }
            other => panic!("expected cursor lease declaration, got {other:?}"),
        };

        let clone = fence.clone();
        drop(fence);
        assert!(
            receiver.try_recv().is_err(),
            "a borrowed fence clone must keep its protocol lease alive"
        );
        drop(clone);
        assert_eq!(
            receiver.try_recv().expect("cursor lease release"),
            RendererOutputTransportMessage::CursorLeaseReleased { stream, lease_id }
        );
    }

    #[test]
    fn merged_fence_retains_each_independent_transport_lease() {
        let stream =
            RendererOutputStreamIdentity::new_page_for_protocol_test(PageId::new_for_testing(7));
        let (transport, mut receiver) = crate::runtime::renderer_output_transport_channel();
        let first = RendererOutputFence::declare(
            RendererOutputCursor::new_for_test(stream, 2),
            Some(transport.clone()),
        );
        let second = RendererOutputFence::declare(
            RendererOutputCursor::new_for_test(stream, 3),
            Some(transport),
        );
        let first_lease = match receiver.try_recv().expect("first lease declaration") {
            RendererOutputTransportMessage::CursorLeaseDeclared { lease_id, .. } => lease_id,
            other => panic!("expected first cursor lease declaration, got {other:?}"),
        };
        let second_lease = match receiver.try_recv().expect("second lease declaration") {
            RendererOutputTransportMessage::CursorLeaseDeclared { lease_id, .. } => lease_id,
            other => panic!("expected second cursor lease declaration, got {other:?}"),
        };

        let merged = first.latest_in_same_stream(second);
        assert_eq!(merged.cursor().sequence(), 3);
        assert!(
            receiver.try_recv().is_err(),
            "merging must not release either independently declared lease"
        );

        drop(merged);
        let released = [
            match receiver.try_recv().expect("first lease release") {
                RendererOutputTransportMessage::CursorLeaseReleased { lease_id, .. } => lease_id,
                other => panic!("expected first cursor lease release, got {other:?}"),
            },
            match receiver.try_recv().expect("second lease release") {
                RendererOutputTransportMessage::CursorLeaseReleased { lease_id, .. } => lease_id,
                other => panic!("expected second cursor lease release, got {other:?}"),
            },
        ];
        assert!(
            released == [first_lease, second_lease] || released == [second_lease, first_lease],
            "the merged fence must release both independent leases exactly once"
        );
    }
}
