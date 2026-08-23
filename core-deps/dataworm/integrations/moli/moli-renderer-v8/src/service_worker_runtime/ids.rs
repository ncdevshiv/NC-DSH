#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ServiceWorkerRegistrationId(pub(super) u64);

impl ServiceWorkerRegistrationId {
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    pub(crate) fn from_u64_for_binding(value: u64) -> Self {
        Self(value)
    }

    #[cfg(test)]
    pub(crate) fn from_u64_for_test(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ServiceWorkerVersionId(pub(super) u64);

impl ServiceWorkerVersionId {
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    pub(crate) fn from_u64_for_binding(value: u64) -> Self {
        Self(value)
    }

    #[cfg(test)]
    pub(crate) fn from_u64_for_test(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ServiceWorkerClientId(pub(super) u64);

impl ServiceWorkerClientId {
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    pub(crate) fn from_u64_for_worker(value: u64) -> Self {
        Self(value)
    }

    #[cfg(test)]
    pub(crate) fn from_u64_for_test(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ServiceWorkerEventId(pub(super) u64);

impl ServiceWorkerEventId {
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    pub(crate) fn from_u64_for_worker(value: u64) -> Self {
        Self(value)
    }
}
