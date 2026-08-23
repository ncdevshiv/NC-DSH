use std::collections::{HashMap, HashSet};

use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct ImportMap {
    pub(crate) inner: ::import_map::ImportMap,
    pub(crate) integrity: HashMap<Url, String>,
}

#[derive(Debug, Clone, Default)]
pub struct ImportMapRegistryState {
    pub(crate) import_map: ImportMap,
    pub(crate) resolved_module_set: Vec<ResolvedModuleRecord>,
    pub(crate) resolved_module_keys: HashSet<ResolvedModuleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ResolvedModuleRecord {
    pub(crate) base_url: String,
    pub(crate) normalized_specifier: String,
    pub(crate) specifier_url: Option<Url>,
}

impl Default for ImportMap {
    fn default() -> Self {
        Self {
            inner: ::import_map::ImportMap::new(
                Url::parse("about:blank").expect("about:blank should be a valid URL"),
            ),
            integrity: HashMap::new(),
        }
    }
}
