use super::*;

mod form;
mod live;
mod table;

impl DomHost {
    pub fn collect_matching_elements(
        &self,
        root: DomHandle,
        include_root: bool,
        mut predicate: impl FnMut(DomHandle) -> bool,
    ) -> Vec<DomHandle> {
        if self.node(root).is_none() {
            return Vec::new();
        }

        let mut stack = Vec::new();
        if include_root {
            stack.push(root);
        } else {
            stack.extend(self.child_handles_reversed(root));
        }

        let mut out = Vec::new();
        while let Some(handle) = stack.pop() {
            if self.node(handle).and_then(Node::as_element).is_some() && predicate(handle) {
                out.push(handle);
            }
            stack.extend(self.child_handles_reversed(handle));
        }
        out
    }

    pub fn child_element_nodes(&self, root: DomHandle) -> Vec<DomHandle> {
        self.dom.child_element_nodes(root)
    }

    pub fn is_html_element_named(&self, handle: DomHandle, local_name: &str) -> bool {
        self.node(handle)
            .is_some_and(|node| node.is_html_element_named(local_name))
    }

    pub fn is_script_element(&self, handle: DomHandle) -> bool {
        self.node(handle).is_some_and(Node::is_script_element)
    }
}
