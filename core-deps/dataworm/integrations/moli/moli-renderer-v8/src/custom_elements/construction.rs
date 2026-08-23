use crate::document_runtime::DomHandle;
use std::collections::HashSet;

struct PendingConstruction {
    constructor: v8::Global<v8::Function>,
    wrapper: Option<v8::Global<v8::Object>>,
    handle: DomHandle,
    kind: PendingConstructionKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingConstructionKind {
    ExistingElementUpgrade,
    SynchronousCreateElement,
}

pub(crate) enum PendingCustomElementConstruction<'s> {
    Wrapper(v8::Local<'s, v8::Object>, DomHandle),
    AlreadyConstructed(DomHandle),
}

#[derive(Default)]
pub(super) struct CustomElementConstructionStack {
    stack: Vec<PendingConstruction>,
    constructing_handles: HashSet<DomHandle>,
    failed_handles: HashSet<DomHandle>,
}

impl CustomElementConstructionStack {
    pub(super) fn begin_existing_element_upgrade(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        constructor: v8::Local<'_, v8::Function>,
        wrapper: v8::Local<'_, v8::Object>,
        handle: DomHandle,
    ) {
        self.begin_with_kind(
            scope,
            constructor,
            wrapper,
            handle,
            PendingConstructionKind::ExistingElementUpgrade,
        );
    }

    pub(super) fn begin_synchronous_create_element(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        constructor: v8::Local<'_, v8::Function>,
        wrapper: v8::Local<'_, v8::Object>,
        handle: DomHandle,
    ) {
        self.begin_with_kind(
            scope,
            constructor,
            wrapper,
            handle,
            PendingConstructionKind::SynchronousCreateElement,
        );
    }

    fn begin_with_kind(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        constructor: v8::Local<'_, v8::Function>,
        wrapper: v8::Local<'_, v8::Object>,
        handle: DomHandle,
        kind: PendingConstructionKind,
    ) {
        self.constructing_handles.insert(handle);
        self.stack.push(PendingConstruction {
            constructor: v8::Global::new(scope, constructor),
            wrapper: Some(v8::Global::new(scope, wrapper)),
            handle,
            kind,
        });
    }

    pub(super) fn discard(&mut self, handle: DomHandle) {
        self.constructing_handles.remove(&handle);
        if let Some(index) = self
            .stack
            .iter()
            .rposition(|pending| pending.handle == handle)
        {
            self.stack.remove(index);
        }
    }

    pub(super) fn take_pending_wrapper_for<'s>(
        &mut self,
        scope: &mut v8::PinScope<'s, '_>,
        new_target: v8::Local<'_, v8::Function>,
    ) -> Option<PendingCustomElementConstruction<'s>> {
        let index = self.stack.iter().rposition(|pending| {
            let constructor = v8::Local::new(scope, &pending.constructor);
            constructor.strict_equals(new_target.into())
        })?;
        let pending = &mut self.stack[index];
        let Some(wrapper) = pending.wrapper.take() else {
            return Some(PendingCustomElementConstruction::AlreadyConstructed(
                pending.handle,
            ));
        };
        let wrapper = v8::Local::new(scope, &wrapper);
        let handle = pending.handle;
        if pending.kind == PendingConstructionKind::SynchronousCreateElement {
            self.stack.remove(index);
        }
        Some(PendingCustomElementConstruction::Wrapper(wrapper, handle))
    }

    pub(super) fn has_pending_wrapper_for(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        new_target: v8::Local<'_, v8::Function>,
    ) -> bool {
        self.stack.iter().any(|pending| {
            let constructor = v8::Local::new(scope, &pending.constructor);
            constructor.strict_equals(new_target.into())
        })
    }

    pub(super) fn is_pending_handle(&self, handle: DomHandle) -> bool {
        self.constructing_handles.contains(&handle)
    }

    pub(super) fn is_already_constructed(&self, handle: DomHandle) -> bool {
        self.stack
            .iter()
            .any(|pending| pending.handle == handle && pending.wrapper.is_none())
    }

    pub(super) fn mark_failed(&mut self, handle: DomHandle) {
        self.failed_handles.insert(handle);
    }

    pub(super) fn clear_failed(&mut self, handle: DomHandle) {
        self.failed_handles.remove(&handle);
    }

    pub(super) fn is_failed(&self, handle: DomHandle) -> bool {
        self.failed_handles.contains(&handle)
    }

    pub(super) fn owns_handle(&self, handle: DomHandle) -> bool {
        self.constructing_handles.contains(&handle)
            || self.failed_handles.contains(&handle)
            || self.stack.iter().any(|pending| pending.handle == handle)
    }
}
