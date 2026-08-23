use super::{
    registry::{
        CustomElementAdoptionPlan, CustomElementRegistryAssociation, CustomElementRegistryKey,
        RegistryAssociationRetarget,
    },
    registry_adoption_callbacks::adoption_callback_targets,
    traversal::shadow_including_child_handles,
};
use crate::{document_runtime::DomHandle, dom::native::Node, native_bridge::JsContextHost};

fn adoption_plan_before_adoption(
    host_ptr: *mut JsContextHost,
    root: DomHandle,
    new_document: DomHandle,
) -> CustomElementAdoptionPlan {
    let host = unsafe { &*host_ptr };
    CustomElementAdoptionPlan {
        targets: adoption_callback_targets(host.dom_host(), root, new_document),
        registry_retargets: registry_association_retargets_before_adoption(
            host_ptr,
            root,
            new_document,
        ),
    }
}

pub(crate) fn adoption_plan_for_roots_before_adoption(
    host_ptr: *mut JsContextHost,
    roots: &[DomHandle],
    new_document: DomHandle,
) -> CustomElementAdoptionPlan {
    let mut plan = CustomElementAdoptionPlan::default();
    for &root in roots {
        plan.extend(adoption_plan_before_adoption(host_ptr, root, new_document));
    }
    plan
}

fn registry_association_retargets_before_adoption(
    host_ptr: *mut JsContextHost,
    root: DomHandle,
    new_document: DomHandle,
) -> Vec<RegistryAssociationRetarget> {
    let mut retargets = Vec::new();
    let preserve_same_document_inherited =
        should_preserve_same_document_registry_association(host_ptr, root);
    collect_registry_association_retargets_before_adoption(
        host_ptr,
        root,
        new_document,
        preserve_same_document_inherited,
        false,
        &mut retargets,
    );
    retargets
}

fn should_preserve_same_document_registry_association(
    host_ptr: *mut JsContextHost,
    root: DomHandle,
) -> bool {
    let host = unsafe { &*host_ptr }.dom_host();
    host.node(root)
        .and_then(Node::parent_node)
        .is_some_and(|parent| {
            host.is_shadow_root(parent)
                || !host.node(parent).is_some_and(Node::is_document_fragment)
        })
}

fn collect_registry_association_retargets_before_adoption(
    host_ptr: *mut JsContextHost,
    root: DomHandle,
    new_document: DomHandle,
    preserve_same_document_inherited: bool,
    preserve_null_shadow_registry: bool,
    retargets: &mut Vec<RegistryAssociationRetarget>,
) {
    let host = unsafe { &*host_ptr };
    let mut stack = vec![(
        root,
        preserve_same_document_inherited,
        preserve_null_shadow_registry,
    )];
    while let Some((handle, preserve_same_document_inherited, preserve_null_shadow_registry)) =
        stack.pop()
    {
        let old_document = host.dom_host().owner_document_handle(handle);
        let current = host.effective_custom_element_registry_association(handle);
        let new_document_default =
            host.default_custom_element_registry_association_for_document(new_document);
        let crosses_document = old_document != Some(new_document);
        let preserves_null_shadow_registry = preserve_null_shadow_registry
            || host
                .dom_host()
                .shadow_root_uses_null_custom_element_registry(handle)
                .unwrap_or(false);
        let association = match current {
            CustomElementRegistryAssociation::Registry(CustomElementRegistryKey::Scoped(_)) => {
                current
            }
            CustomElementRegistryAssociation::Null if preserves_null_shadow_registry => {
                CustomElementRegistryAssociation::Null
            }
            _ if crosses_document => new_document_default,
            CustomElementRegistryAssociation::Registry(
                CustomElementRegistryKey::Global | CustomElementRegistryKey::Child(_),
            ) if current != new_document_default => new_document_default,
            _ => current,
        };
        let has_explicit_association = host.custom_element_registry_association(handle).is_some();
        let should_retarget =
            crosses_document || preserve_same_document_inherited || has_explicit_association;
        if should_retarget {
            retargets.push(RegistryAssociationRetarget {
                handle,
                association,
            });
        }
        let children = shadow_including_child_handles(host_ptr, handle);
        let preserve_descendants =
            preserve_same_document_inherited || has_explicit_association || should_retarget;
        stack.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, preserve_descendants, preserves_null_shadow_registry)),
        );
    }
}
