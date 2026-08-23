use serde::Deserialize;

use crate::devtools_runtime::DevToolsDomNodeReference;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct NodeReferenceParams {
    #[serde(default)]
    pub(super) node_id: Option<u32>,
    #[serde(default)]
    pub(super) backend_node_id: Option<u32>,
    #[serde(default)]
    pub(super) object_id: Option<String>,
}

pub(super) fn devtools_node_reference_from_ids(
    node_id: Option<u32>,
    backend_node_id: Option<u32>,
) -> Option<DevToolsDomNodeReference> {
    node_id
        .map(DevToolsDomNodeReference::FrontendNodeId)
        .or_else(|| backend_node_id.map(DevToolsDomNodeReference::BackendNodeId))
}
