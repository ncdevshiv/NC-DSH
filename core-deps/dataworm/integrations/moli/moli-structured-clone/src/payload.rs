/// Serialized structured clone payload plus transferred host-object metadata.
///
/// `PortId` is generic so the payload crate does not need to own MessagePort id
/// allocation or depend on the MessagePort registry crate. Moli currently
/// uses `u64` ids.
#[derive(Clone, Debug, Default)]
pub struct StructuredCloneBytes<PortId = u64> {
    /// Wire bytes produced by the embedding engine's structured clone encoder.
    pub wire_bytes: Vec<u8>,
    /// ArrayBuffer transfers captured before detaching the sender-side buffers.
    pub transferred_array_buffers: Vec<TransferredArrayBuffer>,
    /// MessagePort ids transferred with the payload.
    pub transferred_message_ports: Vec<PortId>,
}

/// Sender-side bytes for a transferred ArrayBuffer.
#[derive(Clone, Debug)]
pub struct TransferredArrayBuffer {
    /// Transfer id used by the embedding engine's structured clone wire format.
    pub transfer_id: u32,
    /// Captured buffer bytes.
    pub bytes: Vec<u8>,
}
