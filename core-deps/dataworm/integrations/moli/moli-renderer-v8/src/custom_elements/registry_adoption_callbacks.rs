use super::registry::AdoptionCallbackTarget;
use crate::{
    document_runtime::DomHandle,
    dom::native::{DomHost, Node},
};

pub(super) fn adoption_callback_targets(
    host: &DomHost,
    root: DomHandle,
    new_document: DomHandle,
) -> Vec<AdoptionCallbackTarget> {
    let mut targets = Vec::new();
    let mut stack = vec![root];
    while let Some(handle) = stack.pop() {
        if let Some(old_document) = host.node(handle).and_then(Node::owner_document)
            && old_document != new_document
        {
            targets.push(AdoptionCallbackTarget {
                handle,
                old_document,
                new_document,
            });
        }
        let children = host.child_handles(handle).collect::<Vec<_>>();
        stack.extend(children.into_iter().rev());
        if let Some(shadow_root) = host.shadow_root_handle(handle) {
            stack.push(shadow_root);
        }
    }
    targets
}
