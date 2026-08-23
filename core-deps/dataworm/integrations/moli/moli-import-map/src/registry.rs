use url::Url;

use crate::helpers::normalize_module_specifier;
use crate::types::{ImportMapRegistryState, ResolvedModuleRecord};

impl ImportMapRegistryState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn register_import_map(
        &mut self,
        source: &str,
        base_url: &Url,
    ) -> std::result::Result<(), String> {
        self.import_map
            .register_from_json(source, base_url, &self.resolved_module_set)?;
        Ok(())
    }

    pub fn resolve_module_specifier(
        &mut self,
        specifier: &str,
        base_url: &Url,
    ) -> std::result::Result<Url, String> {
        let (normalized_specifier, as_url) = normalize_module_specifier(specifier, base_url);
        let resolved = self
            .import_map
            .resolve(specifier, base_url)?
            .expect("import_map::ImportMap::resolve should return a URL or an error");
        self.record_resolved_module(base_url, &normalized_specifier, as_url.as_ref());
        Ok(resolved)
    }

    pub fn resolve_module_integrity(&self, url: &Url) -> Option<String> {
        self.import_map.resolve_integrity(url)
    }

    fn record_resolved_module(
        &mut self,
        base_url: &Url,
        normalized_specifier: &str,
        specifier_url: Option<&Url>,
    ) {
        let record = ResolvedModuleRecord {
            base_url: base_url.as_str().to_owned(),
            normalized_specifier: normalized_specifier.to_owned(),
            specifier_url: specifier_url.cloned(),
        };
        if self.resolved_module_keys.insert(record.clone()) {
            self.resolved_module_set.push(record);
        }
    }
}
