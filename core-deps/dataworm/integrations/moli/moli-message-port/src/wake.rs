use crate::MessagePortId;

/// Owner wake target for a port with pending work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessagePortWake<O> {
    /// Port whose owner should be scheduled.
    pub port_id: MessagePortId,
    /// Embedding-owned wake handle.
    pub owner: O,
}
