use super::insertion_plan::TreeInsertionPlan;
use crate::{
    custom_elements,
    document_runtime::{DocumentRuntime, DomHandle},
    dom::native::{DomHost, Node},
    native_bridge::JsContextHost,
};

#[derive(Clone, Debug, Default)]
pub(in crate::document_runtime) struct TreeAdoptionPlan {
    roots: Vec<DomHandle>,
    new_document: Option<DomHandle>,
    previous_owner_documents: Vec<(DomHandle, Option<DomHandle>)>,
    custom_elements: custom_elements::CustomElementAdoptionPlan,
}

impl TreeAdoptionPlan {
    pub(in crate::document_runtime) fn before_adoption(
        dom_host: &DomHost,
        host_ptr: *mut JsContextHost,
        roots: &[DomHandle],
        new_document: DomHandle,
        collect_custom_elements: bool,
    ) -> Self {
        let previous_owner_documents = roots
            .iter()
            .copied()
            .map(|root| (root, dom_host.node(root).and_then(Node::owner_document)))
            .collect();
        let custom_elements = if collect_custom_elements {
            custom_elements::adoption_plan_for_roots_before_adoption(host_ptr, roots, new_document)
        } else {
            custom_elements::CustomElementAdoptionPlan::default()
        };
        Self {
            roots: roots.to_vec(),
            new_document: Some(new_document),
            previous_owner_documents,
            custom_elements,
        }
    }

    pub(in crate::document_runtime) fn root(&self) -> Option<DomHandle> {
        self.roots.first().copied()
    }

    pub(super) fn has_targets(&self) -> bool {
        self.custom_elements.has_targets()
    }

    pub(super) fn has_registry_retargets_without_adoption(&self) -> bool {
        self.custom_elements
            .has_registry_retargets_without_adoption()
    }

    pub(in crate::document_runtime) fn custom_elements(
        &self,
    ) -> &custom_elements::CustomElementAdoptionPlan {
        &self.custom_elements
    }

    pub(in crate::document_runtime) fn previous_owner_document_for(
        &self,
        root: DomHandle,
    ) -> Option<DomHandle> {
        self.previous_owner_documents
            .iter()
            .find_map(|(candidate, previous)| (*candidate == root).then_some(*previous))
            .flatten()
    }

    pub(in crate::document_runtime) fn new_document(&self) -> Option<DomHandle> {
        self.new_document
    }
}

impl DocumentRuntime {
    pub(super) fn tree_adoption_plan_before_insert(
        &self,
        host_ptr: *mut JsContextHost,
        roots: &[DomHandle],
        parent: DomHandle,
    ) -> TreeAdoptionPlan {
        let Some(new_document) = self.document_for_insertion_parent(parent) else {
            return TreeAdoptionPlan::default();
        };
        TreeAdoptionPlan::before_adoption(&self.dom_host, host_ptr, roots, new_document, true)
    }

    fn document_for_insertion_parent(&self, parent: DomHandle) -> Option<DomHandle> {
        if self.dom_host.node(parent).is_some_and(Node::is_document) {
            return Some(parent);
        }
        self.dom_host.owner_document_handle(parent)
    }

    pub(super) fn sync_shadow_root_adopted_style_sheets_after_insertion_adoption(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        host_ptr: *mut JsContextHost,
        insertion_plan: &TreeInsertionPlan<'_>,
    ) {
        let Some(new_document) = insertion_plan.adoption.new_document() else {
            return;
        };
        if unsafe { &*host_ptr }
            .child_browsing_context_host_for_document_handle(new_document)
            .is_none()
        {
            return;
        }
        let runtime = unsafe { &mut *host_ptr };
        for &root in insertion_plan.insertion_roots {
            if !insertion_plan
                .adoption
                .previous_owner_document_for(root)
                .is_some_and(|previous| previous != new_document)
            {
                continue;
            }
            for shadow_root in runtime.shadow_roots_in_subtree(root) {
                crate::native_bridge::element::clear_shadow_root_adopted_style_sheets(
                    scope,
                    runtime,
                    shadow_root,
                );
            }
            runtime.note_style_subtree_context_change(root);
        }
    }
}
