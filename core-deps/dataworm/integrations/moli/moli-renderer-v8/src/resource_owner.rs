use std::{
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_RESOURCE_OWNER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ResourceOwnerId(u64);

impl ResourceOwnerId {
    pub(crate) fn new() -> Self {
        Self(
            NEXT_RESOURCE_OWNER_ID
                .fetch_add(1, Ordering::Relaxed)
                .max(1),
        )
    }
}

pub(crate) fn install_resource_owner_for_context(
    context: v8::Local<'_, v8::Context>,
    owner_id: ResourceOwnerId,
) {
    let _previous = context.set_slot(Rc::new(owner_id));
}

pub(crate) fn current_resource_owner_id(
    scope: &mut v8::PinScope<'_, '_>,
) -> Option<ResourceOwnerId> {
    scope
        .get_current_context()
        .get_slot::<ResourceOwnerId>()
        .as_deref()
        .copied()
}
