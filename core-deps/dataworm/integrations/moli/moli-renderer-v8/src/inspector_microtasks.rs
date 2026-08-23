/// Runs one V8 Inspector dispatch with the policy expected by Inspector's
/// internal `MicrotasksScope(kRunMicrotasks)` boundaries.
///
/// Page and worker isolates otherwise use `Explicit` so their owners decide
/// when ordinary page microtasks run. Inspector temporarily needs `Scoped` so
/// protocol-owned promise reactions run before Inspector releases its local
/// roots. Restoring the previous policy before the embedder's explicit
/// checkpoint also keeps nested dispatches and non-Inspector execution on the
/// owner policy.
pub(crate) fn with_scoped_inspector_microtasks<R>(
    isolate: &mut v8::Isolate,
    dispatch: impl FnOnce() -> R,
) -> R {
    let previous = isolate.get_microtasks_policy();
    isolate.set_microtasks_policy(v8::MicrotasksPolicy::Scoped);
    let restore = InspectorMicrotasksPolicyRestore { isolate, previous };
    let result = dispatch();
    drop(restore);
    result
}

struct InspectorMicrotasksPolicyRestore<'a> {
    isolate: &'a mut v8::Isolate,
    previous: v8::MicrotasksPolicy,
}

impl Drop for InspectorMicrotasksPolicyRestore<'_> {
    fn drop(&mut self) {
        self.isolate.set_microtasks_policy(self.previous);
    }
}
