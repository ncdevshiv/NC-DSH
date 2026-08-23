use std::sync::{Arc, Weak};

use moli_core::page::RendererServiceWorkerRunIdentity;

/// Stable lifetime of one renderer-owned ServiceWorker version target.
///
/// Chromium keeps one DevTools agent host for a service-worker version across
/// worker process restarts. Moli mirrors that responsibility here: the
/// target state uniquely owns this strong scope, while prepared output carries
/// only a weak identity. Version destruction moves the strong scope into an
/// ordered retirement output so already accepted events can drain before the
/// target becomes permanently stale.
#[derive(Debug)]
pub(crate) struct TargetServiceWorkerVersionScope {
    inner: Arc<TargetServiceWorkerVersionScopeInner>,
}

#[derive(Debug)]
struct TargetServiceWorkerVersionScopeInner;

/// Exact protocol projection of one renderer ServiceWorker version.
///
/// Raw version and target ids are intentionally insufficient: both are
/// connection-local values which can outlive an async capture boundary. The
/// weak scope distinguishes this exact target incarnation from any later
/// registry entry with identical scalar ids.
#[derive(Clone, Debug)]
pub(crate) struct TargetServiceWorkerVersionIdentity {
    browser_context_id: String,
    renderer_registration_id: u64,
    renderer_version_id: u64,
    target_id: String,
    scope: Weak<TargetServiceWorkerVersionScopeInner>,
}

/// Move-owned authority to publish and retire one version target.
#[derive(Debug)]
pub(crate) struct TargetServiceWorkerVersionRetirement {
    identity: TargetServiceWorkerVersionIdentity,
    scope: TargetServiceWorkerVersionScope,
}

/// Stable lifetime of one protocol session attached to a ServiceWorker target.
///
/// This scope spans worker stop/start cycles, but not ordinary session detach
/// or version destruction. It is therefore deliberately separate from the
/// per-run scope below.
#[derive(Debug)]
pub(crate) struct TargetServiceWorkerProtocolAttachmentScope {
    inner: Arc<TargetServiceWorkerProtocolAttachmentScopeInner>,
}

#[derive(Debug)]
struct TargetServiceWorkerProtocolAttachmentScopeInner;

/// Exact protocol attachment to one ServiceWorker version target.
#[derive(Clone, Debug)]
pub(crate) struct TargetServiceWorkerProtocolAttachmentIdentity {
    version: TargetServiceWorkerVersionIdentity,
    session_id: String,
    scope: Weak<TargetServiceWorkerProtocolAttachmentScopeInner>,
}

/// Move-owned authority to emit the exact detach and then expire the session.
#[derive(Debug)]
pub(crate) struct TargetServiceWorkerProtocolAttachmentRetirement {
    identity: TargetServiceWorkerProtocolAttachmentIdentity,
    scope: TargetServiceWorkerProtocolAttachmentScope,
}

/// Stable lifetime of one concrete ServiceWorker execution run.
///
/// A version target can own many runs over time, but at most one live run
/// scope. Stopping the worker moves this scope into an ordered retirement
/// output. Historical output accepted before the stop remains valid until that
/// retirement drains; output arriving after retirement cannot bind to a later
/// run even if the version and protocol session are unchanged.
#[derive(Debug)]
pub(crate) struct TargetServiceWorkerRunScope {
    inner: Arc<TargetServiceWorkerRunScopeInner>,
    renderer_run: RendererServiceWorkerRunIdentity,
}

#[derive(Debug)]
struct TargetServiceWorkerRunScopeInner;

/// Exact renderer execution run for one ServiceWorker version.
///
/// `renderer_run` is the opaque identity created by the renderer authority.
/// The protocol scope projects that exact run into one target version; it
/// cannot manufacture or reopen a run from scalar transport data.
#[derive(Clone, Debug)]
pub(crate) struct TargetServiceWorkerRunIdentity {
    version: TargetServiceWorkerVersionIdentity,
    renderer_run: RendererServiceWorkerRunIdentity,
    scope: Weak<TargetServiceWorkerRunScopeInner>,
}

/// Move-owned authority to finish output from one worker run and retire it.
#[derive(Debug)]
pub(crate) struct TargetServiceWorkerRunRetirement {
    identity: TargetServiceWorkerRunIdentity,
    scope: TargetServiceWorkerRunScope,
}

/// One protocol session observing one exact ServiceWorker execution run.
///
/// Runtime inspector, console, exception and fetch output must satisfy both
/// independent lifetimes: the DevTools session is still attached to this
/// version target, and the renderer output still belongs to this run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TargetServiceWorkerRuntimeAttachmentIdentity {
    attachment: TargetServiceWorkerProtocolAttachmentIdentity,
    run: TargetServiceWorkerRunIdentity,
}

impl TargetServiceWorkerVersionScope {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(TargetServiceWorkerVersionScopeInner),
        }
    }

    pub(crate) fn bind(
        &self,
        browser_context_id: impl Into<String>,
        renderer_registration_id: u64,
        renderer_version_id: u64,
        target_id: impl Into<String>,
    ) -> TargetServiceWorkerVersionIdentity {
        TargetServiceWorkerVersionIdentity {
            browser_context_id: browser_context_id.into(),
            renderer_registration_id,
            renderer_version_id,
            target_id: target_id.into(),
            scope: Arc::downgrade(&self.inner),
        }
    }

    pub(crate) fn into_retirement(
        self,
        identity: TargetServiceWorkerVersionIdentity,
    ) -> TargetServiceWorkerVersionRetirement {
        assert!(
            self.observes(&identity),
            "service-worker version retirement must consume its own live identity"
        );
        TargetServiceWorkerVersionRetirement {
            identity,
            scope: self,
        }
    }

    fn observes(&self, identity: &TargetServiceWorkerVersionIdentity) -> bool {
        identity
            .scope
            .upgrade()
            .is_some_and(|observed| Arc::ptr_eq(&self.inner, &observed))
    }
}

impl TargetServiceWorkerVersionIdentity {
    pub(crate) fn browser_context_id(&self) -> &str {
        &self.browser_context_id
    }

    pub(crate) fn target_id(&self) -> &str {
        &self.target_id
    }

    pub(crate) fn is_current(&self) -> bool {
        self.scope.upgrade().is_some()
    }
}

impl PartialEq for TargetServiceWorkerVersionIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.browser_context_id == other.browser_context_id
            && self.renderer_registration_id == other.renderer_registration_id
            && self.renderer_version_id == other.renderer_version_id
            && self.target_id == other.target_id
            && Weak::ptr_eq(&self.scope, &other.scope)
    }
}

impl Eq for TargetServiceWorkerVersionIdentity {}

impl TargetServiceWorkerVersionRetirement {
    pub(crate) fn identity(&self) -> &TargetServiceWorkerVersionIdentity {
        &self.identity
    }

    pub(crate) fn is_current(&self) -> bool {
        self.scope.observes(&self.identity)
    }

    pub(crate) fn retire(self) {}
}

impl PartialEq for TargetServiceWorkerVersionRetirement {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for TargetServiceWorkerVersionRetirement {}

impl TargetServiceWorkerProtocolAttachmentScope {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(TargetServiceWorkerProtocolAttachmentScopeInner),
        }
    }

    pub(crate) fn bind(
        &self,
        version: TargetServiceWorkerVersionIdentity,
        session_id: impl Into<String>,
    ) -> TargetServiceWorkerProtocolAttachmentIdentity {
        assert!(
            version.is_current(),
            "service-worker attachment must bind a live version target"
        );
        TargetServiceWorkerProtocolAttachmentIdentity {
            version,
            session_id: session_id.into(),
            scope: Arc::downgrade(&self.inner),
        }
    }

    pub(crate) fn into_retirement(
        self,
        identity: TargetServiceWorkerProtocolAttachmentIdentity,
    ) -> TargetServiceWorkerProtocolAttachmentRetirement {
        assert!(
            self.observes(&identity),
            "service-worker attachment retirement must consume its own live identity"
        );
        TargetServiceWorkerProtocolAttachmentRetirement {
            identity,
            scope: self,
        }
    }

    fn observes(&self, identity: &TargetServiceWorkerProtocolAttachmentIdentity) -> bool {
        identity.version.is_current()
            && identity
                .scope
                .upgrade()
                .is_some_and(|observed| Arc::ptr_eq(&self.inner, &observed))
    }
}

impl TargetServiceWorkerProtocolAttachmentIdentity {
    pub(crate) fn version(&self) -> &TargetServiceWorkerVersionIdentity {
        &self.version
    }

    pub(crate) fn browser_context_id(&self) -> &str {
        self.version.browser_context_id()
    }

    pub(crate) fn target_id(&self) -> &str {
        self.version.target_id()
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn is_current(&self) -> bool {
        self.version.is_current() && self.scope.upgrade().is_some()
    }
}

impl PartialEq for TargetServiceWorkerProtocolAttachmentIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.session_id == other.session_id
            && Weak::ptr_eq(&self.scope, &other.scope)
    }
}

impl Eq for TargetServiceWorkerProtocolAttachmentIdentity {}

impl TargetServiceWorkerProtocolAttachmentRetirement {
    pub(crate) fn identity(&self) -> &TargetServiceWorkerProtocolAttachmentIdentity {
        &self.identity
    }

    pub(crate) fn is_current(&self) -> bool {
        self.scope.observes(&self.identity)
    }

    pub(crate) fn retire(self) {}
}

impl PartialEq for TargetServiceWorkerProtocolAttachmentRetirement {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for TargetServiceWorkerProtocolAttachmentRetirement {}

impl TargetServiceWorkerRunScope {
    pub(super) fn new(renderer_run: RendererServiceWorkerRunIdentity) -> Self {
        Self {
            inner: Arc::new(TargetServiceWorkerRunScopeInner),
            renderer_run,
        }
    }

    pub(super) fn bind(
        &self,
        version: TargetServiceWorkerVersionIdentity,
    ) -> TargetServiceWorkerRunIdentity {
        assert!(
            version.is_current(),
            "service-worker run must bind a live version target"
        );
        TargetServiceWorkerRunIdentity {
            version,
            renderer_run: self.renderer_run.clone(),
            scope: Arc::downgrade(&self.inner),
        }
    }

    pub(super) fn renderer_run(&self) -> &RendererServiceWorkerRunIdentity {
        &self.renderer_run
    }

    pub(crate) fn into_retirement(
        self,
        identity: TargetServiceWorkerRunIdentity,
    ) -> TargetServiceWorkerRunRetirement {
        assert!(
            self.observes(&identity),
            "service-worker run retirement must consume its own live identity"
        );
        TargetServiceWorkerRunRetirement {
            identity,
            scope: self,
        }
    }

    fn observes(&self, identity: &TargetServiceWorkerRunIdentity) -> bool {
        identity.version.is_current()
            && identity.renderer_run == self.renderer_run
            && identity
                .scope
                .upgrade()
                .is_some_and(|observed| Arc::ptr_eq(&self.inner, &observed))
    }
}

impl TargetServiceWorkerRunIdentity {
    pub(crate) fn version(&self) -> &TargetServiceWorkerVersionIdentity {
        &self.version
    }

    pub(crate) fn is_current(&self) -> bool {
        self.version.is_current() && self.scope.upgrade().is_some()
    }

    pub(super) fn renderer_run(&self) -> &RendererServiceWorkerRunIdentity {
        &self.renderer_run
    }
}

impl PartialEq for TargetServiceWorkerRunIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.renderer_run == other.renderer_run
            && Weak::ptr_eq(&self.scope, &other.scope)
    }
}

impl Eq for TargetServiceWorkerRunIdentity {}

impl TargetServiceWorkerRunRetirement {
    pub(crate) fn identity(&self) -> &TargetServiceWorkerRunIdentity {
        &self.identity
    }

    pub(crate) fn is_current(&self) -> bool {
        self.scope.observes(&self.identity)
    }

    pub(crate) fn retire(self) {}
}

impl PartialEq for TargetServiceWorkerRunRetirement {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for TargetServiceWorkerRunRetirement {}

impl TargetServiceWorkerRuntimeAttachmentIdentity {
    pub(crate) fn new(
        attachment: TargetServiceWorkerProtocolAttachmentIdentity,
        run: TargetServiceWorkerRunIdentity,
    ) -> Self {
        assert_eq!(
            attachment.version(),
            run.version(),
            "service-worker runtime attachment must bind one exact version target"
        );
        Self { attachment, run }
    }

    pub(crate) fn attachment(&self) -> &TargetServiceWorkerProtocolAttachmentIdentity {
        &self.attachment
    }

    pub(crate) fn session_id(&self) -> &str {
        self.attachment.session_id()
    }

    pub(crate) fn target_id(&self) -> &str {
        self.attachment.target_id()
    }

    pub(crate) fn run(&self) -> &TargetServiceWorkerRunIdentity {
        &self.run
    }

    pub(crate) fn is_current(&self) -> bool {
        self.attachment.is_current() && self.run.is_current()
    }
}

#[cfg(test)]
mod tests {
    use moli_core::page::RendererServiceWorkerRunIdentity;

    use super::{
        TargetServiceWorkerProtocolAttachmentScope, TargetServiceWorkerRunScope,
        TargetServiceWorkerRuntimeAttachmentIdentity, TargetServiceWorkerVersionScope,
    };

    fn version_binding(
        scope: &TargetServiceWorkerVersionScope,
    ) -> super::TargetServiceWorkerVersionIdentity {
        scope.bind("BID-1", 7, 11, "TID-worker")
    }

    #[test]
    fn ordinary_attachment_drop_expires_only_the_session_binding() {
        let version_scope = TargetServiceWorkerVersionScope::new();
        let version = version_binding(&version_scope);
        let attachment_scope = TargetServiceWorkerProtocolAttachmentScope::new();
        let attachment = attachment_scope.bind(version.clone(), "SID-worker");

        drop(attachment_scope);

        assert!(version.is_current());
        assert!(!attachment.is_current());
    }

    #[test]
    fn run_retirement_does_not_expire_the_version_or_attachment() {
        let version_scope = TargetServiceWorkerVersionScope::new();
        let version = version_binding(&version_scope);
        let attachment_scope = TargetServiceWorkerProtocolAttachmentScope::new();
        let attachment = attachment_scope.bind(version.clone(), "SID-worker");
        let run_scope = TargetServiceWorkerRunScope::new(RendererServiceWorkerRunIdentity::fresh());
        let run = run_scope.bind(version);
        let runtime =
            TargetServiceWorkerRuntimeAttachmentIdentity::new(attachment.clone(), run.clone());
        let retirement = run_scope.into_retirement(run);

        assert!(runtime.is_current());
        retirement.retire();

        assert!(!runtime.is_current());
        assert!(attachment.is_current());
    }

    #[test]
    fn version_retirement_expires_every_nested_identity() {
        let version_scope = TargetServiceWorkerVersionScope::new();
        let version = version_binding(&version_scope);
        let attachment_scope = TargetServiceWorkerProtocolAttachmentScope::new();
        let attachment = attachment_scope.bind(version.clone(), "SID-worker");
        let run_scope = TargetServiceWorkerRunScope::new(RendererServiceWorkerRunIdentity::fresh());
        let run = run_scope.bind(version.clone());
        let retirement = version_scope.into_retirement(version);

        assert!(attachment.is_current());
        assert!(run.is_current());
        retirement.retire();

        assert!(!attachment.is_current());
        assert!(!run.is_current());
    }
}
