use url::Url;

/// The two URLs captured when a string is compiled as a classic script.
///
/// `source_url` is the name exposed by V8 in diagnostics. `module_base_url`
/// is the base used by a dynamic `import()` originating in that compiled
/// string. Keeping them in one value prevents a compiled script from carrying
/// a base URL without a source identity, or silently losing its captured base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CompiledStringProvenance {
    source_url: Url,
    module_base_url: Url,
}

impl CompiledStringProvenance {
    pub(crate) fn new(source_url: Url, module_base_url: Url) -> Self {
        Self {
            source_url,
            module_base_url,
        }
    }

    pub(crate) fn at_url(url: Url) -> Self {
        Self::new(url.clone(), url)
    }

    pub(crate) fn source_url(&self) -> &Url {
        &self.source_url
    }

    pub(crate) fn module_base_url(&self) -> &Url {
        &self.module_base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_keeps_display_and_module_urls_distinct() {
        let source_url = Url::parse("https://example.test/generated/timer.js").unwrap();
        let module_base_url = Url::parse("https://example.test/scripts/entry.js").unwrap();
        let provenance = CompiledStringProvenance::new(source_url.clone(), module_base_url.clone());

        assert_eq!(provenance.source_url(), &source_url);
        assert_eq!(provenance.module_base_url(), &module_base_url);
    }

    #[test]
    fn same_url_provenance_initializes_both_roles() {
        let url = Url::parse("https://example.test/entry.js").unwrap();
        let provenance = CompiledStringProvenance::at_url(url.clone());

        assert_eq!(provenance.source_url(), &url);
        assert_eq!(provenance.module_base_url(), &url);
    }
}
