#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ModuleIdentityHash {
    identity_hash: u32,
}

impl ModuleIdentityHash {
    pub(crate) fn from_raw(identity_hash: u32) -> Self {
        Self { identity_hash }
    }
}
