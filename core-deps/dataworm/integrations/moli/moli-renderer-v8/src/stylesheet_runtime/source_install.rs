use crate::{document_runtime::DomHandle, style_engine::StyloStylesheetSource};

#[derive(Clone, Debug)]
pub(crate) struct PreparedLinkedStylesheetResource {
    source: StyloStylesheetSource,
    import_urls: Vec<url::Url>,
    has_import_rules: bool,
}

impl PreparedLinkedStylesheetResource {
    pub(crate) fn new(
        source: StyloStylesheetSource,
        import_urls: Vec<url::Url>,
        has_import_rules: bool,
    ) -> Self {
        Self {
            source,
            import_urls,
            has_import_rules,
        }
    }

    pub(crate) fn source(&self) -> &StyloStylesheetSource {
        &self.source
    }

    pub(crate) fn import_urls(&self) -> &[url::Url] {
        &self.import_urls
    }

    pub(crate) fn has_import_rules(&self) -> bool {
        self.has_import_rules
    }
}

#[derive(Debug)]
pub(crate) struct InstallLinkedStylesheet {
    owner: DomHandle,
    request_url: url::Url,
    source: PreparedLinkedStylesheetResource,
}

impl InstallLinkedStylesheet {
    pub(crate) fn from_prepared(
        owner: DomHandle,
        request_url: url::Url,
        source: PreparedLinkedStylesheetResource,
    ) -> Self {
        Self {
            owner,
            request_url,
            source,
        }
    }

    pub(crate) fn into_parts(self) -> (DomHandle, url::Url, PreparedLinkedStylesheetResource) {
        (self.owner, self.request_url, self.source)
    }
}
