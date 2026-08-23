/// Internal identity for one opaque-origin execution context.
///
/// Opaque origins often serialize as `"null"`. Routing solely by that string
/// would make unrelated `data:` workers or sandboxed documents share state. The
/// nonce is kept internal to routing/storage keys while JavaScript still sees
/// the public origin string.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct OpaqueOriginNonce(u64);

impl OpaqueOriginNonce {
    /// Create a nonce from a registry- or owner-assigned integer.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the raw nonce value for tests and persistence adapters.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for OpaqueOriginNonce {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}
