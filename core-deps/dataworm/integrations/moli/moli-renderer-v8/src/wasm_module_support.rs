use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::module_runtime::{WasmImportRecord, WasmModuleRecord};

const JS_STRING_CONSTANTS_IMPORT_MODULE: &str = "wasm:js/string-constants";

#[derive(Debug)]
pub(crate) struct WasmModuleMetadata {
    pub(crate) exports: Vec<WasmExportRecord>,
    pub(crate) imports: Vec<WasmImportRecord>,
}

pub(crate) struct CompiledWasmModule<'s> {
    pub(crate) metadata: WasmModuleMetadata,
    pub(crate) module: v8::Local<'s, v8::WasmModuleObject>,
}

pub(crate) struct PreparedWasmModuleRecord {
    pub(crate) record: WasmModuleRecord,
    pub(crate) has_reserved_name_link_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmExportRecord {
    name: String,
    kind: WasmExportKind,
}

impl WasmExportRecord {
    fn new(name: String, kind: WasmExportKind) -> Self {
        Self { name, kind }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn namespace_export_mode(&self) -> WasmNamespaceExportMode {
        match self.kind {
            WasmExportKind::Ordinary => WasmNamespaceExportMode::RawValue,
            WasmExportKind::ImmutableGlobal | WasmExportKind::MutableGlobal => {
                WasmNamespaceExportMode::GlobalSnapshotValue
            }
            WasmExportKind::V128Global => WasmNamespaceExportMode::UninitializedGlobal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WasmExportKind {
    Ordinary,
    ImmutableGlobal,
    MutableGlobal,
    V128Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmNamespaceExportMode {
    RawValue,
    // SyntheticModule currently accepts only ordinary JS values. Non-v128
    // globals are exported as their initial value until V8 exposes a real
    // wasm global live-binding cell.
    GlobalSnapshotValue,
    UninitializedGlobal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmLinkError {
    message: String,
}

impl WasmLinkError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

pub(crate) fn parse_wasm_module_metadata(bytes: &[u8]) -> Result<WasmModuleMetadata> {
    let deps = wasm_dep_analyzer::WasmDeps::parse(
        bytes,
        wasm_dep_analyzer::ParseOptions { skip_types: false },
    )
    .context("failed to analyze WebAssembly module dependencies")?;

    let mut exports = Vec::new();
    let mut seen_exports = HashSet::new();
    for export in deps.exports {
        let name = export.name.to_owned();
        if seen_exports.insert(name.clone()) {
            let kind = wasm_export_kind(&name, &export.export_type)?;
            exports.push(WasmExportRecord::new(name, kind));
        }
    }

    let imports = deps
        .imports
        .into_iter()
        .map(|import| WasmImportRecord::new(import.module.to_owned(), import.name.to_owned()))
        .collect();

    Ok(WasmModuleMetadata { exports, imports })
}

pub(crate) fn compile_wasm_module_with_metadata<'s>(
    scope: &v8::PinScope<'s, '_>,
    bytes: &[u8],
) -> Result<Option<CompiledWasmModule<'s>>> {
    let metadata = parse_wasm_module_metadata(bytes)?;
    // `None` means V8 rejected the module and left the current exception pending
    // for the caller's TryCatch scope to preserve.
    let module = v8::WasmModuleObject::compile_with_options(
        scope,
        bytes,
        wasm_compile_options_for_imports(&metadata.imports),
    );
    Ok(module.map(|module| CompiledWasmModule { metadata, module }))
}

pub(crate) fn prepare_wasm_module_record(
    scope: &v8::PinScope<'_, '_>,
    bytes: &[u8],
) -> Result<Option<PreparedWasmModuleRecord>> {
    let Some(compiled) = compile_wasm_module_with_metadata(scope, bytes)? else {
        return Ok(None);
    };
    let metadata = compiled.metadata;
    let imports = metadata
        .imports
        .into_iter()
        .filter(|import| !is_compile_time_wasm_import(import))
        .collect::<Vec<_>>();
    let exports = metadata.exports;
    let has_reserved_name_link_error = reserved_wasm_name_link_error(&imports, &exports).is_some();
    let compiled_module = Arc::new(compiled.module.get_compiled_module());
    let record = WasmModuleRecord::new(compiled_module, exports, imports);
    Ok(Some(PreparedWasmModuleRecord {
        record,
        has_reserved_name_link_error,
    }))
}

pub(crate) fn v8_exception_message_or(
    scope: &v8::PinScope<'_, '_>,
    exception: Option<v8::Local<'_, v8::Value>>,
    fallback: &str,
) -> String {
    exception
        .and_then(|exception| exception.to_string(scope))
        .map(|message| message.to_rust_string_lossy(scope))
        .unwrap_or_else(|| fallback.to_owned())
}

fn wasm_export_kind(
    name: &str,
    export_type: &wasm_dep_analyzer::ExportType,
) -> Result<WasmExportKind> {
    match export_type {
        wasm_dep_analyzer::ExportType::Global(Ok(global_type)) => {
            if global_type.value_type == wasm_dep_analyzer::ValueType::Unknown {
                anyhow::bail!(
                    "unsupported unknown WebAssembly value type for exported global `{name}`"
                );
            }
            if global_type.value_type == wasm_dep_analyzer::ValueType::V128 {
                Ok(WasmExportKind::V128Global)
            } else if global_type.mutability {
                Ok(WasmExportKind::MutableGlobal)
            } else {
                Ok(WasmExportKind::ImmutableGlobal)
            }
        }
        wasm_dep_analyzer::ExportType::Global(Err(error)) => {
            anyhow::bail!("failed to resolve WebAssembly exported global `{name}`: {error}");
        }
        wasm_dep_analyzer::ExportType::Unknown => {
            anyhow::bail!("unsupported unknown WebAssembly export type for `{name}`");
        }
        _ => Ok(WasmExportKind::Ordinary),
    }
}

pub(crate) fn is_js_string_compile_time_builtin_import(import: &WasmImportRecord) -> bool {
    import.module() == "wasm:js-string"
        && matches!(
            import.name(),
            "cast"
                | "test"
                | "fromCharCode"
                | "fromCodePoint"
                | "charCodeAt"
                | "codePointAt"
                | "length"
                | "concat"
                | "substring"
                | "equals"
                | "compare"
                | "fromCharCodeArray"
                | "intoCharCodeArray"
        )
}

pub(crate) fn wasm_compile_options_for_imports(
    imports: &[WasmImportRecord],
) -> v8::WasmCompileOptions<'_> {
    v8::WasmCompileOptions {
        js_string_builtins: imports.iter().any(is_js_string_compile_time_builtin_import),
        imported_string_constants_module: imported_string_constants_module(imports),
    }
}

fn imported_string_constants_module(imports: &[WasmImportRecord]) -> Option<&str> {
    imports
        .iter()
        .find(|import| import.module() == JS_STRING_CONSTANTS_IMPORT_MODULE)
        .map(WasmImportRecord::module)
}

pub(crate) fn is_compile_time_wasm_import(import: &WasmImportRecord) -> bool {
    is_js_string_compile_time_builtin_import(import)
        || import.module() == JS_STRING_CONSTANTS_IMPORT_MODULE
}

pub(crate) fn wasm_evaluation_import_modules(imports: &[WasmImportRecord]) -> Vec<&str> {
    let mut seen = HashSet::new();
    let mut modules = Vec::new();
    for import in imports {
        if seen.insert(import.module()) {
            modules.push(import.module());
        }
    }
    modules
}

pub(crate) fn reserved_wasm_name_link_error(
    imports: &[WasmImportRecord],
    exports: &[WasmExportRecord],
) -> Option<WasmLinkError> {
    for import in imports {
        if is_reserved_wasm_import_module_name(import.module()) {
            return Some(WasmLinkError::new(format!(
                "WebAssembly module imports reserved module name `{}`",
                import.module()
            )));
        }
        if is_reserved_wasm_import_or_export_name(import.name()) {
            return Some(WasmLinkError::new(format!(
                "WebAssembly module imports reserved name `{}` from `{}`",
                import.name(),
                import.module()
            )));
        }
    }
    for export in exports {
        if is_reserved_wasm_import_or_export_name(export.name()) {
            return Some(WasmLinkError::new(format!(
                "WebAssembly module exports reserved name `{}`",
                export.name()
            )));
        }
    }
    None
}

fn is_reserved_wasm_import_module_name(name: &str) -> bool {
    name.starts_with("wasm-js:")
}

fn is_reserved_wasm_import_or_export_name(name: &str) -> bool {
    name.starts_with("wasm:") || name.starts_with("wasm-js:")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordinary_export(name: &str) -> WasmExportRecord {
        WasmExportRecord::new(name.to_owned(), WasmExportKind::Ordinary)
    }

    fn push_section(module: &mut Vec<u8>, section_id: u8, body: Vec<u8>) {
        assert!(body.len() < 128);
        module.push(section_id);
        module.push(body.len() as u8);
        module.extend(body);
    }

    fn push_name(bytes: &mut Vec<u8>, name: &str) {
        assert!(name.len() < 128);
        bytes.push(name.len() as u8);
        bytes.extend_from_slice(name.as_bytes());
    }

    fn module_with_imported_global_exports(globals: &[(&str, u8, bool)]) -> Vec<u8> {
        assert!(globals.len() < 128);
        let mut bytes = vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
        ];

        let mut imports = Vec::new();
        imports.push(globals.len() as u8);
        for (name, value_type, mutable) in globals {
            push_name(&mut imports, "m");
            push_name(&mut imports, name);
            imports.push(0x03);
            imports.push(*value_type);
            imports.push(u8::from(*mutable));
        }
        push_section(&mut bytes, 0x02, imports);

        let mut exports = Vec::new();
        exports.push(globals.len() as u8);
        for (index, (name, _, _)) in globals.iter().enumerate() {
            assert!(index < 128);
            push_name(&mut exports, name);
            exports.push(0x03);
            exports.push(index as u8);
        }
        push_section(&mut bytes, 0x07, exports);

        bytes
    }

    fn export_kind<'a>(metadata: &'a WasmModuleMetadata, name: &str) -> Option<&'a WasmExportKind> {
        metadata
            .exports
            .iter()
            .find(|export| export.name() == name)
            .map(|export| &export.kind)
    }

    fn export_mode(metadata: &WasmModuleMetadata, name: &str) -> Option<WasmNamespaceExportMode> {
        metadata
            .exports
            .iter()
            .find(|export| export.name() == name)
            .map(WasmExportRecord::namespace_export_mode)
    }

    #[test]
    fn wasm_compile_options_enable_only_used_compile_time_imports() {
        let imports = vec![
            WasmImportRecord::new("wasm:js-string".to_owned(), "cast".to_owned()),
            WasmImportRecord::new("wasm:js/string-constants".to_owned(), "hello".to_owned()),
            WasmImportRecord::new("ordinary".to_owned(), "value".to_owned()),
        ];

        let options = wasm_compile_options_for_imports(&imports);

        assert!(options.js_string_builtins);
        assert_eq!(
            options.imported_string_constants_module,
            Some("wasm:js/string-constants")
        );
        assert!(is_compile_time_wasm_import(&imports[0]));
        assert!(is_compile_time_wasm_import(&imports[1]));
        assert!(!is_compile_time_wasm_import(&imports[2]));
    }

    #[test]
    fn string_constants_do_not_force_js_string_builtins() {
        let imports = vec![WasmImportRecord::new(
            "wasm:js/string-constants".to_owned(),
            "hello".to_owned(),
        )];

        let options = wasm_compile_options_for_imports(&imports);

        assert!(!options.js_string_builtins);
        assert_eq!(
            options.imported_string_constants_module,
            Some("wasm:js/string-constants")
        );
    }

    #[test]
    fn only_known_js_string_builtins_are_compile_time_imports() {
        let imports = [
            WasmImportRecord::new("wasm:js-string".to_owned(), "cast".to_owned()),
            WasmImportRecord::new("wasm:js-string".to_owned(), "newbuiltin".to_owned()),
        ];

        assert!(is_compile_time_wasm_import(&imports[0]));
        assert!(!is_compile_time_wasm_import(&imports[1]));
    }

    #[test]
    fn wasm_evaluation_import_modules_preserves_first_dependency_order() {
        let imports = [
            WasmImportRecord::new("./dep-a.js".to_owned(), "first".to_owned()),
            WasmImportRecord::new("./dep-b.js".to_owned(), "value".to_owned()),
            WasmImportRecord::new("./dep-a.js".to_owned(), "second".to_owned()),
        ];

        assert_eq!(
            wasm_evaluation_import_modules(&imports),
            vec!["./dep-a.js", "./dep-b.js"]
        );
    }

    #[test]
    fn reserved_wasm_export_names_are_reported_as_link_errors() {
        let imports = Vec::new();
        let exports = vec![ordinary_export("wasm-js:invalid")];

        let error = reserved_wasm_name_link_error(&imports, &exports).unwrap();

        assert_eq!(
            error.message(),
            "WebAssembly module exports reserved name `wasm-js:invalid`"
        );
    }

    #[test]
    fn wasm_js_import_module_names_are_reserved() {
        let imports = vec![WasmImportRecord::new(
            "wasm-js:invalid".to_owned(),
            "test".to_owned(),
        )];

        let error = reserved_wasm_name_link_error(&imports, &[]).unwrap();

        assert_eq!(
            error.message(),
            "WebAssembly module imports reserved module name `wasm-js:invalid`"
        );
    }

    #[test]
    fn wasm_global_export_mutability_is_preserved() {
        let mut bytes = vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
            0x06, 0x0b, 0x02, // global section
            0x7f, 0x00, 0x41, 0x07, 0x0b, // immutable i32 global
            0x7f, 0x01, 0x41, 0x08, 0x0b, // mutable i32 global
            0x07, 0x17, 0x02, // export section
        ];
        bytes.push(0x09);
        bytes.extend_from_slice(b"immutable");
        bytes.extend_from_slice(&[0x03, 0x00]);
        bytes.push(0x07);
        bytes.extend_from_slice(b"mutable");
        bytes.extend_from_slice(&[0x03, 0x01]);

        let metadata = parse_wasm_module_metadata(&bytes).unwrap();

        assert_eq!(
            export_kind(&metadata, "immutable"),
            Some(&WasmExportKind::ImmutableGlobal)
        );
        assert_eq!(
            export_kind(&metadata, "mutable"),
            Some(&WasmExportKind::MutableGlobal)
        );
        assert_eq!(
            export_mode(&metadata, "immutable"),
            Some(WasmNamespaceExportMode::GlobalSnapshotValue)
        );
        assert_eq!(
            export_mode(&metadata, "mutable"),
            Some(WasmNamespaceExportMode::GlobalSnapshotValue)
        );
    }

    #[test]
    fn wasm_imported_global_export_mutability_is_preserved() {
        let bytes = module_with_imported_global_exports(&[
            ("imported_immutable", 0x7f, false),
            ("imported_mutable", 0x7f, true),
        ]);

        let metadata = parse_wasm_module_metadata(&bytes).unwrap();

        assert_eq!(
            export_kind(&metadata, "imported_immutable"),
            Some(&WasmExportKind::ImmutableGlobal)
        );
        assert_eq!(
            export_kind(&metadata, "imported_mutable"),
            Some(&WasmExportKind::MutableGlobal)
        );
        assert_eq!(
            export_mode(&metadata, "imported_immutable"),
            Some(WasmNamespaceExportMode::GlobalSnapshotValue)
        );
        assert_eq!(
            export_mode(&metadata, "imported_mutable"),
            Some(WasmNamespaceExportMode::GlobalSnapshotValue)
        );
    }

    #[test]
    fn wasm_v128_global_exports_are_preserved_for_namespace_tdz() {
        let mut bytes = vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
            0x06, 0x16, 0x01, // global section
            0x7b, 0x00, 0xfd, 0x0c, // immutable v128 global, v128.const
            0x00, 0x00, 0x00, 0x00, // lanes
            0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x0b, // end
            0x07, 0x08, 0x01, // export section
        ];
        bytes.push(0x04);
        bytes.extend_from_slice(b"v128");
        bytes.extend_from_slice(&[0x03, 0x00]);

        let metadata = parse_wasm_module_metadata(&bytes).unwrap();

        assert_eq!(
            export_kind(&metadata, "v128"),
            Some(&WasmExportKind::V128Global)
        );
        assert_eq!(
            export_mode(&metadata, "v128"),
            Some(WasmNamespaceExportMode::UninitializedGlobal)
        );
    }

    #[test]
    fn wasm_imported_v128_global_exports_are_preserved_for_namespace_tdz() {
        let bytes = module_with_imported_global_exports(&[("imported_v128", 0x7b, false)]);

        let metadata = parse_wasm_module_metadata(&bytes).unwrap();

        assert_eq!(
            export_kind(&metadata, "imported_v128"),
            Some(&WasmExportKind::V128Global)
        );
        assert_eq!(
            export_mode(&metadata, "imported_v128"),
            Some(WasmNamespaceExportMode::UninitializedGlobal)
        );
    }

    #[test]
    fn wasm_unknown_export_type_fails_closed() {
        let mut bytes = vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
        ];
        let mut exports = vec![0x01];
        push_name(&mut exports, "mystery");
        exports.extend_from_slice(&[0xff, 0x00]);
        push_section(&mut bytes, 0x07, exports);

        let error = parse_wasm_module_metadata(&bytes).unwrap_err().to_string();

        assert_eq!(
            error,
            "unsupported unknown WebAssembly export type for `mystery`"
        );
    }

    #[test]
    fn wasm_unresolved_global_export_type_fails_closed() {
        let mut bytes = vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
        ];
        let mut exports = vec![0x01];
        push_name(&mut exports, "missing_global");
        exports.extend_from_slice(&[0x03, 0x00]);
        push_section(&mut bytes, 0x07, exports);

        let error = parse_wasm_module_metadata(&bytes).unwrap_err().to_string();

        assert_eq!(
            error,
            "failed to resolve WebAssembly exported global `missing_global`: unresolved export type"
        );
    }

    #[test]
    fn wasm_unknown_global_value_type_fails_closed() {
        let mut bytes = vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
            0x06, 0x06, 0x01, // global section
            0x6e, 0x00, 0x41, 0x00, 0x0b, // unknown value type global
        ];
        let mut exports = vec![0x01];
        push_name(&mut exports, "unknown_global");
        exports.extend_from_slice(&[0x03, 0x00]);
        push_section(&mut bytes, 0x07, exports);

        let error = parse_wasm_module_metadata(&bytes).unwrap_err().to_string();

        assert_eq!(
            error,
            "unsupported unknown WebAssembly value type for exported global `unknown_global`"
        );
    }
}
