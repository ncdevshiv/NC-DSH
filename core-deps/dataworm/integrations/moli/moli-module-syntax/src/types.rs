// Current module-runtime contract:
//
// 1. We do not execute source as native ESM inside V8. Instead we translate a narrow, explicit
//    subset of module syntax into classic-script-friendly code plus a small live-binding shim.
// 2. The rewrite pipeline is intentionally split into three conceptual phases:
//    - AST statement lowering (`import`, `export`, ordinary body statements)
//    - AST special-form rewriting inside executable code (`import.meta.url`,
//      `import.meta.resolve(...)`, `import(...)`)
//    - setup/export synthesis (`__lm_imports`, module registry entries, live bindings)
// 3. The supported syntax surface is deliberately small and is guarded by tests below:
//    - static default / named / namespace / side-effect-only imports
//    - export const / let / var / function / function* / class / export list / export-all
//      re-exports
//    - export default expression / function / function* / class
//    - `import.meta` / `import.meta.url`
//    - `import.meta.resolve(...)`
//    - statically foldable string dynamic import, both bare and `await import(...)`
//      for JavaScript/JSON modules; CSS dynamic imports stay in source for V8's host hook
//    - static import/export headers with ignored import-attributes syntax
//      (`with { ... }` / legacy `assert { ... }`)
// 4. Unsupported forms should fail loudly during rewrite instead of being half-transformed:
//    - non-static non-CSS dynamic import specifiers
// 5. Oxc parsing is kept behind this crate so renderer/runtime code owns Moli semantics
//    without depending directly on parser allocation and AST lifetime details.

#[derive(Debug, Clone)]
pub struct ModuleNamedBinding {
    pub imported_name: String,
    pub local_name: String,
}

#[derive(Debug, Clone)]
pub struct ParsedModuleStaticImport {
    pub specifier: String,
    pub import_type: Option<String>,
    pub default_binding: Option<String>,
    pub namespace_binding: Option<String>,
    pub named_bindings: Vec<ModuleNamedBinding>,
}

#[derive(Debug, Clone)]
pub struct ParsedModuleExportList {
    pub specifier: Option<String>,
    pub import_type: Option<String>,
    pub bindings: Vec<ModuleExportBinding>,
}

#[derive(Debug, Clone)]
pub struct ParsedModuleExportAll {
    pub specifier: String,
    pub import_type: Option<String>,
    pub namespace_export_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModuleExportBinding {
    pub local_name: String,
    pub export_name: String,
}

#[derive(Debug, Clone)]
pub struct ParsedExportConst {
    pub bindings: Vec<ParsedExportVariableBinding>,
}

#[derive(Debug, Clone)]
pub struct ParsedExportVariable {
    pub bindings: Vec<ParsedExportVariableBinding>,
}

#[derive(Debug, Clone)]
pub struct ParsedExportVariableBinding {
    pub local_name: String,
    pub export_name: String,
}

#[derive(Debug, Clone)]
pub struct ParsedExportedFunction {
    pub local_name: String,
    pub export_name: String,
}

#[derive(Debug, Clone)]
pub struct ParsedExportedClass {
    pub local_name: String,
    pub export_name: String,
}

#[derive(Debug, Clone)]
pub struct ParsedExportDefaultDeclaration {
    pub declaration_source: ModuleAstSourceFragment,
    pub is_anonymous: bool,
    pub local_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModulePredeclaredExportSurface {
    pub explicit_export_names: Vec<String>,
    pub has_export_star: bool,
}

#[derive(Debug, Clone)]
pub struct ModuleAstLowering {
    pub predeclared_export_surface: ModulePredeclaredExportSurface,
    pub statements: Vec<ModuleAstStatement>,
}

#[derive(Debug, Clone)]
pub struct ModuleAstSourceFragment {
    pub source: String,
    pub span: ModuleAstSpan,
    pub special_forms: ModuleSpecialFormRewriteSites,
    pub contains_top_level_await: bool,
}

#[derive(Debug, Clone)]
pub enum ModuleAstStatement {
    Empty,
    StaticImport(ParsedModuleStaticImport),
    ExportedFunction {
        export: ParsedExportedFunction,
        local_source: ModuleAstSourceFragment,
    },
    ExportedClass {
        export: ParsedExportedClass,
        local_source: ModuleAstSourceFragment,
    },
    ExportConst {
        export: ParsedExportConst,
        local_source: ModuleAstSourceFragment,
    },
    ExportVariable {
        export: ParsedExportVariable,
        local_source: ModuleAstSourceFragment,
    },
    ExportDefaultDeclaration(ParsedExportDefaultDeclaration),
    ExportDefaultExpr(ModuleAstSourceFragment),
    ExportList(ParsedModuleExportList),
    ExportAll(ParsedModuleExportAll),
    Body(ModuleAstSourceFragment),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicImportRewriteKind {
    AwaitedNamespace,
    Promise,
}

#[derive(Debug, Clone)]
pub struct DynamicImportRewriteSite {
    pub specifier: String,
    pub resolve_import_meta_first: bool,
    pub import_type: Option<String>,
    pub replace_start: usize,
    pub replace_end: usize,
    pub kind: DynamicImportRewriteKind,
}

#[derive(Debug, Clone)]
pub struct ImportMetaResolveRewriteSite {
    pub specifier: String,
    pub replace_start: usize,
    pub replace_end: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleSpecialFormRewriteSites {
    pub import_metas: Vec<ModuleAstSpan>,
    pub import_meta_urls: Vec<ModuleAstSpan>,
    pub import_meta_resolves: Vec<ImportMetaResolveRewriteSite>,
    pub dynamic_imports: Vec<DynamicImportRewriteSite>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleAstSpan {
    pub start: u32,
    pub end: u32,
}
