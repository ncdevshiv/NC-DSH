use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct DomStorageId {
    #[serde(default)]
    pub(super) security_origin: Option<String>,
    #[serde(default)]
    pub(super) storage_key: Option<String>,
    pub(super) is_local_storage: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StorageIdParams {
    pub(super) storage_id: DomStorageId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoveItemParams {
    pub(super) storage_id: DomStorageId,
    pub(super) key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetItemParams {
    pub(super) storage_id: DomStorageId,
    pub(super) key: String,
    pub(super) value: String,
}
