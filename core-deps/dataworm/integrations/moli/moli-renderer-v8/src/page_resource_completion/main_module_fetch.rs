use url::Url;

/// Protocol attribution captured where a main-Document module request starts.
///
/// Parser-owned graphs, runtime-created graphs, and modulepreload use distinct
/// executable targets, but their Network projection needs the same immutable
/// producer snapshot. This value deliberately contains no execution authority:
/// neither URL may be used to select or authorize a current Document owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MainModuleFetchNetworkAttribution {
    document_url: Url,
    request_url: Url,
}

impl MainModuleFetchNetworkAttribution {
    pub(crate) fn new(document_url: Url, request_url: Url) -> Self {
        Self {
            document_url,
            request_url,
        }
    }

    pub(crate) fn document_url(&self) -> &Url {
        &self.document_url
    }

    pub(crate) fn request_url(&self) -> &Url {
        &self.request_url
    }
}
