use serde_json::{Value, json};

use moli_page_types::{DocumentNodeSnapshot, DocumentSnapshotNodeId, MAX_DOM_OUTPUT_TREE_DEPTH};

pub fn frontend_node_id_for_snapshot(snapshot: &DocumentNodeSnapshot) -> Option<u32> {
    snapshot.frontend_node_id
}

fn frontend_parent_node_id_for_snapshot(snapshot: &DocumentNodeSnapshot) -> Option<u32> {
    snapshot.parent_frontend_node_id
}

pub fn backend_node_id_for_snapshot(snapshot: &DocumentNodeSnapshot) -> Option<u32> {
    snapshot.backend_node_id
}

pub fn node_snapshot_to_cdp(
    snapshot: &DocumentNodeSnapshot,
    top_snapshot_node_id: Option<DocumentSnapshotNodeId>,
    top_frame_id: Option<&str>,
) -> Option<Value> {
    node_snapshot_to_cdp_with_limit(
        snapshot,
        top_snapshot_node_id,
        top_frame_id,
        MAX_DOM_OUTPUT_TREE_DEPTH,
    )
}

#[derive(Clone, Copy)]
enum CdpNodeOutputRelation {
    Child,
    ShadowRoot,
    PseudoElement,
    ContentDocument,
}

struct CdpNodeOutputFrame<'a> {
    snapshot: &'a DocumentNodeSnapshot,
    payload: serde_json::Map<String, Value>,
    remaining_tree_depth: usize,
    next_child_index: usize,
    next_shadow_root_index: usize,
    next_pseudo_element_index: usize,
    content_document_processed: bool,
    children: Vec<Value>,
    shadow_roots: Vec<Value>,
    pseudo_elements: Vec<Value>,
    template_content: Option<Value>,
    content_document: Option<Value>,
    relation_to_parent: Option<CdpNodeOutputRelation>,
}

impl<'a> CdpNodeOutputFrame<'a> {
    fn new(
        snapshot: &'a DocumentNodeSnapshot,
        top_snapshot_node_id: Option<DocumentSnapshotNodeId>,
        top_frame_id: Option<&str>,
        remaining_tree_depth: usize,
        relation_to_parent: Option<CdpNodeOutputRelation>,
    ) -> Option<Self> {
        let template_content = snapshot.template_content().and_then(|content| {
            node_snapshot_base_payload(content, top_snapshot_node_id, top_frame_id)
                .map(Value::Object)
        });
        Some(Self {
            snapshot,
            payload: node_snapshot_base_payload(snapshot, top_snapshot_node_id, top_frame_id)?,
            remaining_tree_depth,
            next_child_index: 0,
            next_shadow_root_index: 0,
            next_pseudo_element_index: 0,
            content_document_processed: false,
            children: Vec::new(),
            shadow_roots: Vec::new(),
            pseudo_elements: Vec::new(),
            template_content,
            content_document: None,
            relation_to_parent,
        })
    }

    fn finish(mut self) -> Value {
        if !self.children.is_empty() {
            self.payload
                .insert("children".to_owned(), Value::Array(self.children));
        }
        if !self.shadow_roots.is_empty() {
            self.payload
                .insert("shadowRoots".to_owned(), Value::Array(self.shadow_roots));
        }
        if !self.pseudo_elements.is_empty() {
            self.payload.insert(
                "pseudoElements".to_owned(),
                Value::Array(self.pseudo_elements),
            );
        }
        if let Some(template_content) = self.template_content {
            self.payload
                .insert("templateContent".to_owned(), template_content);
        }
        if let Some(content_document) = self.content_document {
            self.payload
                .insert("contentDocument".to_owned(), content_document);
        }
        Value::Object(self.payload)
    }
}

pub fn node_snapshot_to_cdp_with_limit(
    snapshot: &DocumentNodeSnapshot,
    top_snapshot_node_id: Option<DocumentSnapshotNodeId>,
    top_frame_id: Option<&str>,
    remaining_tree_depth: usize,
) -> Option<Value> {
    let mut stack = vec![CdpNodeOutputFrame::new(
        snapshot,
        top_snapshot_node_id,
        top_frame_id,
        remaining_tree_depth,
        None,
    )?];

    while let Some(frame) = stack.last_mut() {
        let next_tree_depth = frame.remaining_tree_depth.checked_sub(1);
        if let Some(next_tree_depth) = next_tree_depth {
            if let Some(child) = frame.snapshot.children.get(frame.next_child_index) {
                frame.next_child_index += 1;
                if let Some(child_frame) = CdpNodeOutputFrame::new(
                    child,
                    top_snapshot_node_id,
                    top_frame_id,
                    next_tree_depth,
                    Some(CdpNodeOutputRelation::Child),
                ) {
                    stack.push(child_frame);
                }
                continue;
            }
            if let Some(shadow_root) = frame
                .snapshot
                .shadow_roots
                .get(frame.next_shadow_root_index)
            {
                frame.next_shadow_root_index += 1;
                if let Some(shadow_root_frame) = CdpNodeOutputFrame::new(
                    shadow_root,
                    top_snapshot_node_id,
                    top_frame_id,
                    next_tree_depth,
                    Some(CdpNodeOutputRelation::ShadowRoot),
                ) {
                    stack.push(shadow_root_frame);
                }
                continue;
            }
            if let Some(pseudo_element) = frame
                .snapshot
                .pseudo_elements
                .get(frame.next_pseudo_element_index)
            {
                frame.next_pseudo_element_index += 1;
                if let Some(pseudo_element_frame) = CdpNodeOutputFrame::new(
                    pseudo_element,
                    top_snapshot_node_id,
                    top_frame_id,
                    next_tree_depth,
                    Some(CdpNodeOutputRelation::PseudoElement),
                ) {
                    stack.push(pseudo_element_frame);
                }
                continue;
            }
            if !frame.content_document_processed {
                frame.content_document_processed = true;
                if let Some(content_document) = frame.snapshot.content_document()
                    && let Some(content_document_frame) = CdpNodeOutputFrame::new(
                        content_document,
                        top_snapshot_node_id,
                        top_frame_id,
                        next_tree_depth,
                        Some(CdpNodeOutputRelation::ContentDocument),
                    )
                {
                    stack.push(content_document_frame);
                    continue;
                }
            }
        }

        let Some(frame) = stack.pop() else {
            break;
        };
        let relation_to_parent = frame.relation_to_parent;
        let value = frame.finish();
        let Some(parent) = stack.last_mut() else {
            return Some(value);
        };
        if let Some(relation_to_parent) = relation_to_parent {
            match relation_to_parent {
                CdpNodeOutputRelation::Child => parent.children.push(value),
                CdpNodeOutputRelation::ShadowRoot => parent.shadow_roots.push(value),
                CdpNodeOutputRelation::PseudoElement => parent.pseudo_elements.push(value),
                CdpNodeOutputRelation::ContentDocument => {
                    parent.content_document = Some(value);
                }
            }
        }
    }

    None
}

pub fn node_snapshot_base_payload(
    snapshot: &DocumentNodeSnapshot,
    top_snapshot_node_id: Option<DocumentSnapshotNodeId>,
    top_frame_id: Option<&str>,
) -> Option<serde_json::Map<String, Value>> {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "nodeId".to_owned(),
        json!(frontend_node_id_for_snapshot(snapshot)?),
    );
    payload.insert(
        "backendNodeId".to_owned(),
        json!(backend_node_id_for_snapshot(snapshot)?),
    );
    payload.insert("nodeType".to_owned(), json!(snapshot.node_type));
    payload.insert("nodeName".to_owned(), json!(snapshot.node_name));
    payload.insert("localName".to_owned(), json!(snapshot.local_name));
    payload.insert("nodeValue".to_owned(), json!(snapshot.node_value));
    payload.insert("childNodeCount".to_owned(), json!(snapshot.child_count));
    payload.insert("documentURL".to_owned(), json!(snapshot.document_url));
    payload.insert("baseURL".to_owned(), json!(snapshot.base_url));
    payload.insert("xmlVersion".to_owned(), json!(""));

    if let Some(parent_id) = frontend_parent_node_id_for_snapshot(snapshot) {
        payload.insert("parentId".to_owned(), json!(parent_id));
    }

    if let Some(name) = snapshot.document_type_name.as_ref() {
        payload.insert("name".to_owned(), json!(name));
        payload.insert("publicId".to_owned(), json!(snapshot.public_id));
        payload.insert("systemId".to_owned(), json!(snapshot.system_id));
    }

    if !snapshot.attributes.is_empty()
        || snapshot.pseudo_type.is_some()
        || (snapshot.is_element && snapshot.inspector_identity.is_some())
    {
        let mut attributes = Vec::with_capacity(snapshot.attributes.len() * 2);
        for attribute in &snapshot.attributes {
            attributes.push(json!(attribute.local_name));
            attributes.push(json!(attribute.value));
        }
        payload.insert("attributes".to_owned(), Value::Array(attributes));
    }

    if let Some(shadow_root_type) = snapshot.shadow_root_type.as_ref() {
        payload.insert("shadowRootType".to_owned(), json!(shadow_root_type));
    }
    if let Some(pseudo_type) = snapshot.pseudo_type.as_ref() {
        payload.insert("pseudoType".to_owned(), json!(pseudo_type));
    }

    if snapshot.is_element
        && matches!(snapshot.local_name.as_str(), "iframe" | "frame")
        && let Some(frame_id) = snapshot.frame_id.as_ref()
    {
        payload.insert("frameId".to_owned(), json!(frame_id));
    } else if snapshot.is_element
        && snapshot.local_name == "html"
        && (Some(snapshot.node_id) == top_snapshot_node_id
            || snapshot.parent_id == top_snapshot_node_id)
        && let Some(frame_id) = top_frame_id
    {
        payload.insert("frameId".to_owned(), json!(frame_id));
    }

    Some(payload)
}

pub fn collect_flattened_node_snapshot(
    snapshot: &DocumentNodeSnapshot,
    top_snapshot_node_id: DocumentSnapshotNodeId,
    top_frame_id: Option<&str>,
    nodes: &mut Vec<Value>,
) {
    let mut stack = vec![snapshot];
    while let Some(snapshot) = stack.pop() {
        if let Some(node) =
            node_snapshot_to_cdp_shallow(snapshot, top_snapshot_node_id, top_frame_id)
        {
            nodes.push(node);
        }
        stack.extend(snapshot.shadow_roots.iter().rev());
        stack.extend(snapshot.children.iter().rev());
    }
}

fn node_snapshot_to_cdp_shallow(
    snapshot: &DocumentNodeSnapshot,
    top_snapshot_node_id: DocumentSnapshotNodeId,
    top_frame_id: Option<&str>,
) -> Option<Value> {
    Some(Value::Object(node_snapshot_base_payload(
        snapshot,
        Some(top_snapshot_node_id),
        top_frame_id,
    )?))
}
