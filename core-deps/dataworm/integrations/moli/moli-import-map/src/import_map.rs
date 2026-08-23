use std::collections::HashMap;

use serde_json::{Map, Value};
use url::Url;

use crate::helpers::{
    import_map_entry_value, import_map_error_message, resolve_url_like_module_specifier,
    resolved_module_matches_rule, scope_matches_record,
};
use crate::types::{ImportMap, ResolvedModuleRecord};

impl ImportMap {
    pub(crate) fn register_from_json(
        &mut self,
        source: &str,
        base_url: &Url,
        resolved_modules: &[ResolvedModuleRecord],
    ) -> std::result::Result<(), String> {
        let source_value: Value = serde_json::from_str(source)
            .map_err(|error| format!("failed to parse import map: {error}"))?;
        let integrity = parse_integrity_map(&source_value, base_url)?;
        let parsed = ::import_map::parse_from_value(base_url.clone(), source_value)
            .map_err(|error| format!("failed to parse import map: {error}"))?;

        let mut merged = self.to_json_object();
        self.merge_existing_and_new_import_map(&mut merged, &parsed.import_map, resolved_modules);
        let reparsed = ::import_map::parse_from_value(base_url.clone(), Value::Object(merged))
            .map_err(|error| format!("failed to merge import map: {error}"))?;
        self.inner = reparsed.import_map;
        self.merge_integrity(integrity);
        Ok(())
    }

    pub(crate) fn resolve_integrity(&self, url: &Url) -> Option<String> {
        self.integrity.get(url).cloned()
    }

    fn merge_integrity(&mut self, integrity: HashMap<Url, String>) {
        for (url, value) in integrity {
            self.integrity.entry(url).or_insert(value);
        }
    }

    fn merge_existing_and_new_import_map(
        &self,
        merged: &mut Map<String, Value>,
        new_import_map: &::import_map::ImportMap,
        resolved_modules: &[ResolvedModuleRecord],
    ) {
        let imports = merged
            .entry("imports")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(imports) = imports {
            for entry in new_import_map.imports().entries() {
                if resolved_modules.iter().any(|record| {
                    resolved_module_matches_rule(entry.key, entry.key.ends_with('/'), record)
                }) {
                    continue;
                }
                imports
                    .entry(entry.key.to_owned())
                    .or_insert_with(|| import_map_entry_value(entry.value));
            }
        }

        let scopes = merged
            .entry("scopes")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(scopes) = scopes {
            for scope in new_import_map.scopes() {
                let Ok(scope_prefix) = Url::parse(scope.key) else {
                    continue;
                };
                let scoped_resolved_modules = resolved_modules
                    .iter()
                    .filter(|record| scope_matches_record(&scope_prefix, record))
                    .collect::<Vec<_>>();
                let scope_imports = scopes
                    .entry(scope.key.to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                let Value::Object(scope_imports) = scope_imports else {
                    continue;
                };
                for entry in scope.imports.entries() {
                    if scoped_resolved_modules.iter().any(|record| {
                        resolved_module_matches_rule(entry.key, entry.key.ends_with('/'), record)
                    }) {
                        continue;
                    }
                    scope_imports
                        .entry(entry.key.to_owned())
                        .or_insert_with(|| import_map_entry_value(entry.value));
                }
            }
        }
    }

    pub(crate) fn resolve(
        &self,
        specifier: &str,
        base_url: &Url,
    ) -> std::result::Result<Option<Url>, String> {
        match self.inner.resolve(specifier, base_url) {
            Ok(url) => Ok(Some(url)),
            Err(error) => Err(import_map_error_message(error)),
        }
    }

    pub(crate) fn to_json_object(&self) -> Map<String, Value> {
        let mut root = Map::new();
        let mut imports = Map::new();
        for entry in self.inner.imports().entries() {
            imports.insert(entry.key.to_owned(), import_map_entry_value(entry.value));
        }
        if !imports.is_empty() {
            root.insert("imports".to_owned(), Value::Object(imports));
        }

        let mut scopes = Map::new();
        for scope in self.inner.scopes() {
            let mut scope_imports = Map::new();
            for entry in scope.imports.entries() {
                scope_imports.insert(entry.key.to_owned(), import_map_entry_value(entry.value));
            }
            if !scope_imports.is_empty() {
                scopes.insert(scope.key.to_owned(), Value::Object(scope_imports));
            }
        }
        if !scopes.is_empty() {
            root.insert("scopes".to_owned(), Value::Object(scopes));
        }
        root
    }
}

fn parse_integrity_map(
    parsed: &Value,
    base_url: &Url,
) -> std::result::Result<HashMap<Url, String>, String> {
    let Some(integrity) = parsed.get("integrity") else {
        return Ok(HashMap::new());
    };
    let Value::Object(integrity) = integrity else {
        return Err(
            "failed to parse import map: \"integrity\" top-level key must be a JSON object"
                .to_owned(),
        );
    };
    let mut normalized = HashMap::new();
    for (raw_key, value) in integrity {
        let Some(url) = resolve_url_like_module_specifier(raw_key, base_url) else {
            continue;
        };
        let Some(value) = value.as_str() else {
            continue;
        };
        normalized.insert(url, value.to_owned());
    }
    Ok(normalized)
}
