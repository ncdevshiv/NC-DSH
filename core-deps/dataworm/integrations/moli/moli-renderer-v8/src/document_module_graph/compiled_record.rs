#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ModuleCompiledRecordId(u32);

impl ModuleCompiledRecordId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("native module compiled record index exceeded u32::MAX"))
    }

    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}
