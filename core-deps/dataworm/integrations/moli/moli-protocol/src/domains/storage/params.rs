use serde_json::Value;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserContextParam {
    pub(crate) browser_context_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetCookiesParams {
    pub(crate) cookies: Vec<CdpCookieParam>,
    pub(crate) browser_context_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteCookiesParams {
    pub(crate) name: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) domain: Option<String>,
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) partition_key: Option<Value>,
    pub(crate) browser_context_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClearDataForOriginParams {
    pub(crate) origin: String,
    pub(crate) storage_types: String,
    #[serde(default)]
    pub(crate) browser_context_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClearDataForStorageKeyParams {
    pub(crate) storage_key: String,
    pub(crate) storage_types: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetUsageAndQuotaParams {
    pub(crate) origin: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverrideQuotaForOriginParams {
    pub(crate) origin: String,
    #[serde(default)]
    pub(crate) quota_size: Option<f64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetStorageKeyForFrameParams {
    pub(crate) frame_id: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CdpCookieParam {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) url: Option<String>,
    pub(crate) domain: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) secure: Option<bool>,
    #[serde(default)]
    pub(crate) http_only: bool,
    #[serde(default)]
    pub(crate) same_site: Option<String>,
    #[serde(default)]
    pub(crate) priority: Option<String>,
    #[serde(default)]
    pub(crate) source_scheme: Option<String>,
    #[serde(default)]
    pub(crate) source_port: Option<i32>,
    #[serde(default)]
    pub(crate) partition_key: Option<Value>,
    #[serde(default)]
    pub(crate) partition_key_opaque: Option<bool>,
    pub(crate) expires: Option<f64>,
}
