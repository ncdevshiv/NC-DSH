use super::super::{OwnerDispatchScope, RuntimeObservableContextToken};
use super::{
    WindowExecutionContextBinding, WindowExecutionContextOwner,
    binding::WindowExecutionContextLocator,
};

/// Exact LocalDOMWindow target captured when Window-owned work is accepted.
///
/// This deliberately stops at the Window owner rather than a V8 realm token.
/// A keepalive Fetch can outlive its JS realm, while ordinary requests become
/// stale when this LocalWindow is replaced.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct WindowTaskTarget {
    dispatch_scope: OwnerDispatchScope,
    owner: WindowExecutionContextOwner,
}

impl WindowTaskTarget {
    pub(crate) const fn new(
        dispatch_scope: OwnerDispatchScope,
        owner: WindowExecutionContextOwner,
    ) -> Self {
        Self {
            dispatch_scope,
            owner,
        }
    }

    pub(crate) const fn dispatch_scope(self) -> OwnerDispatchScope {
        self.dispatch_scope
    }

    pub(crate) const fn owner(self) -> WindowExecutionContextOwner {
        self.owner
    }
}

/// Realm and LocalWindow coordinates retained by a pending Window Fetch.
///
/// Blink gets both coordinates from the WebIDL receiver: generated bindings
/// use `ScriptState::ForRelevantRealm(v8_receiver)`, while `GlobalFetch`
/// receives that receiver's `LocalDOMWindow`. We retain them separately only
/// because teardown treats them differently, not so callers can freely pair
/// unrelated realms and Windows.
pub(crate) struct WindowFetchContext {
    script_realm: WindowExecutionContextBinding,
    request_target: WindowTaskTarget,
}

impl WindowFetchContext {
    /// Builds both retained coordinates from one authorized receiver realm.
    ///
    /// Keeping this constructor derivational prevents the former
    /// `with_owner()` bug from returning under a new type: callers cannot
    /// freely pair one realm with another Window generation.
    pub(crate) fn from_realm(script_realm: WindowExecutionContextBinding) -> Self {
        let request_target =
            WindowTaskTarget::new(script_realm.dispatch_scope(), script_realm.owner());
        Self {
            script_realm,
            request_target,
        }
    }

    pub(crate) fn script_realm(&self) -> &WindowExecutionContextBinding {
        &self.script_realm
    }

    pub(crate) const fn request_target(&self) -> WindowTaskTarget {
        self.request_target
    }

    pub(crate) fn duplicate(&self, scope: &mut v8::PinScope<'_, '_>) -> Self {
        Self::from_realm(self.script_realm.duplicate(scope))
    }

    pub(crate) fn detached(&self) -> DetachedWindowFetchContext {
        DetachedWindowFetchContext::new(self.script_realm.locator(), self.request_target)
    }
}

/// Network-only residue of a keepalive Fetch after JS delivery is detached.
///
/// No V8 global is retained. The realm token remains diagnostic identity only;
/// it must never be used to re-enter a retired realm.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DetachedWindowFetchContext {
    script_realm: WindowExecutionContextLocator,
    request_target: WindowTaskTarget,
}

impl DetachedWindowFetchContext {
    const fn new(
        script_realm: WindowExecutionContextLocator,
        request_target: WindowTaskTarget,
    ) -> Self {
        Self {
            script_realm,
            request_target,
        }
    }

    pub(crate) const fn script_realm_token(self) -> RuntimeObservableContextToken {
        self.script_realm.realm_token()
    }

    pub(crate) const fn request_target(self) -> WindowTaskTarget {
        self.request_target
    }
}
