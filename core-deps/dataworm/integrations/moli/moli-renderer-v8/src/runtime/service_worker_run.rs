use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

/// Opaque identity of one concrete ServiceWorker execution run.
///
/// A stable ServiceWorker version can stop and restart without changing its
/// version id. The renderer therefore creates a fresh identity for every
/// actual run and carries that identity with all run-specific protocol facts.
///
/// This value deliberately exposes no numeric generation. Consumers may
/// compare identities for exact equality, but cannot reconstruct a run from a
/// version id plus a scalar or use ordering to manufacture a newer run.
#[derive(Clone)]
pub struct RendererServiceWorkerRunIdentity {
    inner: Arc<RendererServiceWorkerRunIdentityInner>,
}

#[derive(Debug)]
struct RendererServiceWorkerRunIdentityInner;

impl RendererServiceWorkerRunIdentity {
    /// Creates a fresh renderer-owned run identity.
    ///
    /// Production callers should create this only at the ServiceWorker runtime
    /// authority. The constructor remains public because renderer activity
    /// carriers are shared across internal crates and their boundary tests must
    /// be able to construct exact, non-colliding identities.
    pub fn fresh() -> Self {
        Self {
            inner: Arc::new(RendererServiceWorkerRunIdentityInner),
        }
    }
}

impl fmt::Debug for RendererServiceWorkerRunIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RendererServiceWorkerRunIdentity")
            .finish_non_exhaustive()
    }
}

impl PartialEq for RendererServiceWorkerRunIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for RendererServiceWorkerRunIdentity {}

impl Hash for RendererServiceWorkerRunIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.inner).hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::RendererServiceWorkerRunIdentity;

    #[test]
    fn clones_retain_one_exact_run_identity() {
        let run = RendererServiceWorkerRunIdentity::fresh();

        assert_eq!(run, run.clone());
    }

    #[test]
    fn separate_runs_never_compare_equal() {
        assert_ne!(
            RendererServiceWorkerRunIdentity::fresh(),
            RendererServiceWorkerRunIdentity::fresh()
        );
    }
}
