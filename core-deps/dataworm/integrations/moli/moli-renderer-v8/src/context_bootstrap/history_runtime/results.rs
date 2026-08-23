use crate::native_bridge::PendingNavigationResult;

use super::super::navigation_result::suppress_unhandled_rejection;

pub(super) fn resolve_pending_navigation_committed(
    scope: &mut v8::PinScope<'_, '_>,
    results: &[PendingNavigationResult],
    resolved_value: v8::Local<'_, v8::Value>,
) {
    for result in results {
        let committed_resolver = v8::Local::new(scope, &result.committed_resolver);
        let _ = committed_resolver.resolve(scope, resolved_value);
    }
}

pub(super) fn resolve_pending_navigation_finished(
    scope: &mut v8::PinScope<'_, '_>,
    results: &[PendingNavigationResult],
    resolved_value: v8::Local<'_, v8::Value>,
) {
    for result in results {
        let finished_resolver = v8::Local::new(scope, &result.finished_resolver);
        let _ = finished_resolver.resolve(scope, resolved_value);
    }
}

pub(super) fn resolve_pending_navigation_results(
    scope: &mut v8::PinScope<'_, '_>,
    results: Vec<PendingNavigationResult>,
    resolved_value: v8::Local<'_, v8::Value>,
) {
    resolve_pending_navigation_committed(scope, &results, resolved_value);
    resolve_pending_navigation_finished(scope, &results, resolved_value);
}

pub(in crate::context_bootstrap) fn reject_pending_navigation_results(
    scope: &mut v8::PinScope<'_, '_>,
    results: &[PendingNavigationResult],
    error: v8::Local<'_, v8::Value>,
) {
    reject_pending_navigation_committed(scope, results, error);
    reject_pending_navigation_finished(scope, results, error);
}

pub(super) fn reject_pending_navigation_committed(
    scope: &mut v8::PinScope<'_, '_>,
    results: &[PendingNavigationResult],
    error: v8::Local<'_, v8::Value>,
) {
    for result in results {
        let committed_resolver = v8::Local::new(scope, &result.committed_resolver);
        let _ = committed_resolver.reject(scope, error);
    }
}

pub(super) fn reject_pending_navigation_finished(
    scope: &mut v8::PinScope<'_, '_>,
    results: &[PendingNavigationResult],
    error: v8::Local<'_, v8::Value>,
) {
    for result in results {
        let finished_resolver = v8::Local::new(scope, &result.finished_resolver);
        let _ = finished_resolver.reject(scope, error);
        suppress_unhandled_rejection(scope, finished_resolver.get_promise(scope));
    }
}
