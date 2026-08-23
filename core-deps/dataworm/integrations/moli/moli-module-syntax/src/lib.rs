//! Syntax scanning helpers for legacy module analysis and child script binding discovery.
//!
//! The window/document ESM runtime no longer lowers modules into classic-script
//! shims. Some AST inventory helpers remain while worker/native ESM migration
//! and child script declared-name discovery are being separated.

mod ast_inventory;
mod dynamic_import;
mod exports;
mod imports;
mod types;

pub use types::{
    DynamicImportRewriteKind, DynamicImportRewriteSite, ImportMetaResolveRewriteSite,
    ModuleAstLowering, ModuleAstSourceFragment, ModuleAstSpan, ModuleAstStatement,
    ModuleExportBinding, ModuleNamedBinding, ModulePredeclaredExportSurface,
    ModuleSpecialFormRewriteSites, ParsedExportConst, ParsedExportDefaultDeclaration,
    ParsedExportVariable, ParsedExportVariableBinding, ParsedExportedClass, ParsedExportedFunction,
    ParsedModuleExportAll, ParsedModuleExportList, ParsedModuleStaticImport,
};

pub use ast_inventory::lower_module_source_with_ast_lowering;

pub use exports::{
    parse_script_top_level_assignment_declared_names,
    parse_script_top_level_lexical_declared_names, parse_script_var_and_function_declared_names,
};

#[cfg(test)]
mod tests {
    use crate::ast_inventory::{
        lower_module_source_with_ast, lower_module_source_with_ast_lowering,
    };
    use crate::dynamic_import::collect_module_special_form_rewrite_sites;
    use crate::exports::collect_predeclared_export_surface;
    use crate::{
        DynamicImportRewriteKind, ModuleAstStatement,
        parse_script_top_level_assignment_declared_names,
        parse_script_top_level_lexical_declared_names,
        parse_script_var_and_function_declared_names,
    };

    fn lower_one_module_statement(source: &str) -> ModuleAstStatement {
        let mut statements =
            lower_module_source_with_ast(source).expect("module source should lower");
        assert_eq!(statements.len(), 1);
        statements.remove(0)
    }

    fn lower_default_expression(source: &str) -> String {
        let ModuleAstStatement::ExportDefaultExpr(expression) = lower_one_module_statement(source)
        else {
            panic!("source should lower as a default expression");
        };
        expression.source
    }

    fn lower_default_declaration(source: &str) -> crate::ParsedExportDefaultDeclaration {
        let ModuleAstStatement::ExportDefaultDeclaration(declaration) =
            lower_one_module_statement(source)
        else {
            panic!("source should lower as a default declaration");
        };
        declaration
    }

    #[test]
    fn parses_side_effect_only_static_import() {
        let ModuleAstStatement::StaticImport(import) =
            lower_one_module_statement("import './mod.js';")
        else {
            panic!("source should lower as a static import");
        };
        assert_eq!(import.specifier, "./mod.js");
        assert!(import.default_binding.is_none());
        assert!(import.namespace_binding.is_none());
        assert!(import.named_bindings.is_empty());

        let ModuleAstStatement::StaticImport(compact_import) =
            lower_one_module_statement("import{named as local}from './compact.js';")
        else {
            panic!("compact source should lower as a static import");
        };
        assert_eq!(compact_import.specifier, "./compact.js");
        assert_eq!(compact_import.named_bindings.len(), 1);
        assert_eq!(compact_import.named_bindings[0].imported_name, "named");
        assert_eq!(compact_import.named_bindings[0].local_name, "local");
    }

    #[test]
    fn parses_static_module_headers_with_empty_import_attributes() {
        let ModuleAstStatement::StaticImport(import) =
            lower_one_module_statement("import value, { named } from './mod.js' with {};")
        else {
            panic!("source should lower as a static import");
        };
        assert_eq!(import.specifier, "./mod.js");
        assert_eq!(import.import_type, None);
        assert_eq!(import.default_binding.as_deref(), Some("value"));
        assert_eq!(import.named_bindings.len(), 1);
        assert_eq!(import.named_bindings[0].imported_name, "named");
        assert_eq!(import.named_bindings[0].local_name, "named");

        let ModuleAstStatement::ExportList(export_list) =
            lower_one_module_statement("export { named as renamed } from './mod.js' with {};")
        else {
            panic!("source should lower as an export list");
        };
        assert_eq!(export_list.specifier.as_deref(), Some("./mod.js"));
        assert_eq!(export_list.import_type, None);
        assert_eq!(export_list.bindings.len(), 1);
        assert_eq!(export_list.bindings[0].local_name, "named");
        assert_eq!(export_list.bindings[0].export_name, "renamed");

        let ModuleAstStatement::ExportList(compact_export_list) =
            lower_one_module_statement("export{named as renamed};")
        else {
            panic!("compact source should lower as an export list");
        };
        assert_eq!(compact_export_list.bindings.len(), 1);
        assert_eq!(compact_export_list.bindings[0].local_name, "named");
        assert_eq!(compact_export_list.bindings[0].export_name, "renamed");

        let ModuleAstStatement::ExportAll(export_all) =
            lower_one_module_statement("export * as ns from './mod.js' with {};")
        else {
            panic!("source should lower as an export-all");
        };
        assert_eq!(export_all.specifier, "./mod.js");
        assert_eq!(export_all.import_type, None);
        assert_eq!(export_all.namespace_export_name.as_deref(), Some("ns"));
    }

    #[test]
    fn parses_static_module_headers_with_json_import_attributes() {
        let ModuleAstStatement::StaticImport(import) =
            lower_one_module_statement("import config from './config.json' with { type: 'json' };")
        else {
            panic!("source should lower as a static import");
        };
        assert_eq!(import.specifier, "./config.json");
        assert_eq!(import.import_type.as_deref(), Some("json"));
        assert_eq!(import.default_binding.as_deref(), Some("config"));

        let ModuleAstStatement::StaticImport(legacy) = lower_one_module_statement(
            "import config from './config.json' assert { \"type\": \"json\" };",
        ) else {
            panic!("source should lower as a static import");
        };
        assert_eq!(legacy.specifier, "./config.json");
        assert_eq!(legacy.import_type.as_deref(), Some("json"));
    }

    #[test]
    fn parses_static_module_headers_with_text_import_attributes() {
        let ModuleAstStatement::StaticImport(import) =
            lower_one_module_statement("import content from './file.txt' with { type: 'text' };")
        else {
            panic!("source should lower as a static import");
        };
        assert_eq!(import.specifier, "./file.txt");
        assert_eq!(import.import_type.as_deref(), Some("text"));
        assert_eq!(import.default_binding.as_deref(), Some("content"));
    }

    #[test]
    fn rejects_unknown_static_import_export_attributes() {
        for source in [
            "import config from './config.json' with { integrity: 'sha256-test' };",
            "export { config } from './config.json' with { integrity: 'sha256-test' };",
            "export * from './config.json' with { integrity: 'sha256-test' };",
        ] {
            let error = lower_module_source_with_ast_lowering(source)
                .expect_err("unsupported static import/export attributes should fail lowering");
            assert!(
                error.contains("unsupported import attribute"),
                "unexpected error for {source}: {error}"
            );
        }
    }

    #[test]
    fn exported_variable_local_source_preserves_statement_semicolon() {
        let ModuleAstStatement::ExportConst {
            local_source: const_source,
            ..
        } = lower_one_module_statement("export const value = 1;")
        else {
            panic!("source should lower as exported const");
        };
        assert_eq!(const_source.source, "const value = 1;");

        let ModuleAstStatement::ExportVariable {
            local_source: variable_source,
            ..
        } = lower_one_module_statement("export let value = 1;")
        else {
            panic!("source should lower as exported variable");
        };
        assert_eq!(variable_source.source, "let value = 1;");
    }

    #[test]
    fn lowering_drops_hashbang_trivia_before_first_statement() {
        let lowering = lower_module_source_with_ast_lowering(
            "#!/usr/bin/env node\nconst value = import.meta.url;",
        )
        .expect("hashbang module source should lower");

        assert_eq!(lowering.statements.len(), 1);
        let ModuleAstStatement::Body(body) = &lowering.statements[0] else {
            panic!("hashbang should not become a body statement");
        };
        assert_eq!(body.source, "const value = import.meta.url;");
        assert_eq!(body.special_forms.import_meta_urls.len(), 1);

        let lowering =
            lower_module_source_with_ast_lowering("#!/usr/bin/env node\nimport './dep.js';")
                .expect("hashbang before import should lower");
        assert_eq!(lowering.statements.len(), 1);
        assert!(matches!(
            lowering.statements[0],
            ModuleAstStatement::StaticImport(_)
        ));
    }

    #[test]
    fn lowering_flags_top_level_await_from_oxc_ast() {
        let lowering = lower_module_source_with_ast_lowering(
            [
                "const literal = 'await';",
                "const nested = async () => await load();",
                "function nestedFunction() { return import('./inside-function.js'); }",
                "const templ = `prefix:${await load()}`;",
                "for await (const value of values) { consume(value); }",
                "export const value = await load();",
                "export default await load();",
            ]
            .join("\n")
            .as_str(),
        )
        .expect("module source should lower");

        let ModuleAstStatement::Body(literal) = &lowering.statements[0] else {
            panic!("first statement should lower as body");
        };
        assert!(!literal.contains_top_level_await);

        let ModuleAstStatement::Body(nested) = &lowering.statements[1] else {
            panic!("second statement should lower as body");
        };
        assert!(!nested.contains_top_level_await);

        let ModuleAstStatement::Body(nested_function) = &lowering.statements[2] else {
            panic!("third statement should lower as body");
        };
        assert!(!nested_function.contains_top_level_await);
        assert_eq!(nested_function.special_forms.dynamic_imports.len(), 1);

        let ModuleAstStatement::Body(template) = &lowering.statements[3] else {
            panic!("fourth statement should lower as body");
        };
        assert!(template.contains_top_level_await);

        let ModuleAstStatement::Body(for_await) = &lowering.statements[4] else {
            panic!("fifth statement should lower as body");
        };
        assert!(for_await.contains_top_level_await);

        let ModuleAstStatement::ExportConst { local_source, .. } = &lowering.statements[5] else {
            panic!("sixth statement should lower as exported const");
        };
        assert!(local_source.contains_top_level_await);

        let ModuleAstStatement::ExportDefaultExpr(expression) = &lowering.statements[6] else {
            panic!("seventh statement should lower as default expression");
        };
        assert!(expression.contains_top_level_await);
    }

    #[test]
    fn collects_special_form_rewrite_sites_with_oxc_ast() {
        let source = [
            "const literal = 'import.meta.url import(\"./ignored.js\")';",
            "const templ = `url:${import.meta.url}`;",
            "const resolved = import.meta.resolve(`./nested/` + 'dep' + `.js`);",
            "const promise = import('./lazy.js', { with: { type: 'json' } });",
            "const css = import('./styles.css', { with: { type: 'css' } });",
            "const legacy = import('./legacy.js', { assert: { \"type\": \"json\" } });",
            "const escaped = import('./caf\\u{e9}.js');",
            "const namespace = await /* keep */ import('./awaited.js');",
            "const resolvedImport = await import(import.meta.resolve('./mapped.js'));",
        ]
        .join("\n");

        let sites = collect_module_special_form_rewrite_sites(&source)
            .expect("special forms should collect from AST");

        assert_eq!(sites.import_meta_urls.len(), 1);
        assert_eq!(
            &source
                [sites.import_meta_urls[0].start as usize..sites.import_meta_urls[0].end as usize],
            "import.meta.url"
        );
        assert_eq!(sites.import_metas.len(), 1);
        assert_eq!(
            &source[sites.import_metas[0].start as usize..sites.import_metas[0].end as usize],
            "import.meta"
        );
        assert_eq!(sites.dynamic_imports.len(), 6);
        assert_eq!(sites.dynamic_imports[0].specifier, "./lazy.js");
        assert!(!sites.dynamic_imports[0].resolve_import_meta_first);
        assert_eq!(
            sites.dynamic_imports[0].import_type.as_deref(),
            Some("json")
        );
        assert_eq!(
            sites.dynamic_imports[0].kind,
            DynamicImportRewriteKind::Promise
        );
        assert_eq!(sites.dynamic_imports[1].specifier, "./styles.css");
        assert_eq!(sites.dynamic_imports[1].import_type.as_deref(), Some("css"));
        assert_eq!(sites.dynamic_imports[2].specifier, "./legacy.js");
        assert_eq!(
            sites.dynamic_imports[2].import_type.as_deref(),
            Some("json")
        );
        assert_eq!(sites.dynamic_imports[3].specifier, "./café.js");
        assert_eq!(sites.dynamic_imports[4].specifier, "./awaited.js");
        assert_eq!(
            sites.dynamic_imports[4].kind,
            DynamicImportRewriteKind::AwaitedNamespace
        );
        assert_eq!(
            &source[sites.dynamic_imports[4].replace_start..sites.dynamic_imports[4].replace_end],
            "await /* keep */ import('./awaited.js')"
        );
        assert_eq!(sites.dynamic_imports[5].specifier, "./mapped.js");
        assert!(sites.dynamic_imports[5].resolve_import_meta_first);
        assert_eq!(
            &source[sites.dynamic_imports[5].replace_start..sites.dynamic_imports[5].replace_end],
            "await import(import.meta.resolve('./mapped.js'))"
        );
    }

    #[test]
    fn collects_special_forms_from_expression_fragments() {
        let expression = "({ url: import.meta.url, mod: import('./object.js') })";
        let sites = collect_module_special_form_rewrite_sites(expression)
            .expect("expression fragment should collect through AST wrapper");

        assert_eq!(sites.import_meta_urls.len(), 1);
        assert_eq!(
            &expression
                [sites.import_meta_urls[0].start as usize..sites.import_meta_urls[0].end as usize],
            "import.meta.url"
        );
        assert_eq!(sites.dynamic_imports.len(), 1);
        assert_eq!(sites.dynamic_imports[0].specifier, "./object.js");
    }

    #[test]
    fn lowering_attaches_special_form_sites_to_source_fragments() {
        let lowering = lower_module_source_with_ast_lowering(
            [
                "const url = import.meta.url;",
                "export const dep = await import('./dep.js');",
                "export default import.meta.resolve('./resolved.js');",
            ]
            .join("\n")
            .as_str(),
        )
        .expect("module source should lower");

        let ModuleAstStatement::Body(body) = &lowering.statements[0] else {
            panic!("first statement should lower as body");
        };
        assert_eq!(body.special_forms.import_meta_urls.len(), 1);
        let url_span = body.special_forms.import_meta_urls[0];
        assert_eq!(
            &body.source[url_span.start as usize..url_span.end as usize],
            "import.meta.url"
        );

        let ModuleAstStatement::ExportConst { local_source, .. } = &lowering.statements[1] else {
            panic!("second statement should lower as exported const");
        };
        assert_eq!(local_source.special_forms.dynamic_imports.len(), 1);
        assert_eq!(
            local_source.special_forms.dynamic_imports[0].kind,
            DynamicImportRewriteKind::AwaitedNamespace
        );
        assert_eq!(
            &local_source.source[local_source.special_forms.dynamic_imports[0].replace_start
                ..local_source.special_forms.dynamic_imports[0].replace_end],
            "await import('./dep.js')"
        );

        let ModuleAstStatement::ExportDefaultExpr(expression) = &lowering.statements[2] else {
            panic!("third statement should lower as default expression");
        };
        assert_eq!(expression.special_forms.import_metas.len(), 1);
        assert_eq!(
            &expression.source[expression.special_forms.import_metas[0].start as usize
                ..expression.special_forms.import_metas[0].end as usize],
            "import.meta"
        );
    }

    #[test]
    fn lowering_includes_predeclared_export_surface_from_same_parse() {
        let lowering = lower_module_source_with_ast_lowering(
            "export const value = 1; export * from './x.js';",
        )
        .expect("module source should lower");

        assert_eq!(
            lowering.predeclared_export_surface.explicit_export_names,
            vec!["value"]
        );
        assert!(lowering.predeclared_export_surface.has_export_star);
    }

    #[test]
    fn ignores_special_form_words_inside_trivia_chunks() {
        let trivia = [
            "#!/usr/bin/env node",
            "// import.meta.url",
            "/* import.meta.resolve('./ignored.js') */",
            "// import('./ignored.js')",
        ]
        .join("\n");

        let sites = collect_module_special_form_rewrite_sites(&trivia)
            .expect("trivia-only chunks should not be parsed as executable code");

        assert!(sites.import_meta_urls.is_empty());
        assert!(sites.import_meta_resolves.is_empty());
        assert!(sites.dynamic_imports.is_empty());
    }

    #[test]
    fn rejects_invalid_non_trivia_special_form_fragments() {
        let error = collect_module_special_form_rewrite_sites("import(")
            .expect_err("invalid executable fragments should stay unsupported");

        assert!(error.contains("unsupported module special-form syntax"));
    }

    #[test]
    fn rejects_non_static_special_form_specifiers() {
        let dynamic_error = collect_module_special_form_rewrite_sites("const p = import(name);")
            .expect_err("non-static dynamic import should stay unsupported");
        assert!(dynamic_error.contains("unsupported dynamic import syntax"));

        let css_dynamic = collect_module_special_form_rewrite_sites(
            "const p = import(name, { with: { type: 'css' } });",
        )
        .expect("CSS dynamic import should stay in source for V8's host hook");
        assert!(css_dynamic.dynamic_imports.is_empty());

        let resolve_sites =
            collect_module_special_form_rewrite_sites("const p = import.meta.resolve(name);")
                .expect("dynamic import.meta.resolve should be handled by runtime import.meta");
        assert_eq!(resolve_sites.import_metas.len(), 1);

        let extra_arg_sites = collect_module_special_form_rewrite_sites(
            "const p = import.meta.resolve('./dep.js', './base.js');",
        )
        .expect("multi-argument import.meta.resolve should stay a runtime call");
        assert_eq!(extra_arg_sites.import_metas.len(), 1);
    }

    #[test]
    fn rejects_non_static_or_ambiguous_dynamic_import_options() {
        for source in [
            "const p = import('./dep.js', { ...opts });",
            "const p = import('./dep.js', { ['with']: { type: 'json' } });",
            "const p = import('./dep.js', { unrelated });",
            "const p = import('./dep.js', { with: { type: 'json' }, with: { type: 'json' } });",
            "const p = import('./dep.js', { with: { type: 'json' }, assert: { type: 'json' } });",
            "const p = import('./dep.js', { with: { type: 'json', type: 'css' } });",
            "const p = import('./dep.js', { with: { type: getType() } });",
            "const p = import('./dep.js', { with: { other: 'json' } });",
            "const p = import('./dep.js', { unrelated: 'value' });",
        ] {
            let error = collect_module_special_form_rewrite_sites(source)
                .expect_err("non-static import options should stay unsupported");
            assert!(
                error.contains("unsupported dynamic import syntax"),
                "unexpected error for {source}: {error}"
            );
        }

        let sites = collect_module_special_form_rewrite_sites(
            "const p = import('./dep.js', ({ with: { type: 'json' } }));",
        )
        .expect("static dynamic import options should remain supported");
        assert_eq!(sites.dynamic_imports.len(), 1);
        assert_eq!(
            sites.dynamic_imports[0].import_type.as_deref(),
            Some("json")
        );

        let sites = collect_module_special_form_rewrite_sites(
            "const p = import('./dep.js', { with: {} });",
        )
        .expect("empty static dynamic import attributes should remain supported");
        assert_eq!(sites.dynamic_imports.len(), 1);
        assert_eq!(sites.dynamic_imports[0].import_type, None);
    }

    #[test]
    fn decodes_export_all_namespace_and_string_literals() {
        let ModuleAstStatement::ExportAll(namespace) =
            lower_one_module_statement("export * as \"n\\u0073:name\" from './mod.js';")
        else {
            panic!("source should lower as an export-all");
        };
        assert_eq!(namespace.specifier, "./mod.js");
        assert_eq!(namespace.namespace_export_name.as_deref(), Some("ns:name"));

        let ModuleAstStatement::ExportAll(rocket_namespace) =
            lower_one_module_statement("export * as \"\\uD83D\\uDE80\" from './emoji.js';")
        else {
            panic!("source should lower as an export-all");
        };
        assert_eq!(
            rocket_namespace.namespace_export_name.as_deref(),
            Some("\u{1F680}")
        );
    }

    #[test]
    fn predeclared_export_surface_uses_oxc_ast_for_export_names() {
        let surface = collect_predeclared_export_surface(
            r#"
            export const {alpha, beta: renamed, nested: [gamma]} = source;
            export function ready() {}
            export default class {}
            export { ready as "ready-alias" };
            export * as ns from './namespace.js';
            export * from './all.js';
            "#,
        )
        .expect("module source should parse");

        assert_eq!(
            surface.explicit_export_names,
            vec![
                "alpha".to_owned(),
                "renamed".to_owned(),
                "gamma".to_owned(),
                "ready".to_owned(),
                "default".to_owned(),
                "ready-alias".to_owned(),
                "ns".to_owned(),
            ]
        );
        assert!(surface.has_export_star);
    }

    #[test]
    fn parses_default_exports_with_oxc_ast_shape() {
        let expression = lower_default_expression("export default foo ? bar : baz;");
        assert_eq!(expression, "foo ? bar : baz");

        let anonymous_function = lower_default_declaration("export default function () {}");
        assert_eq!(
            anonymous_function.declaration_source.source,
            "function () {}"
        );
        assert!(anonymous_function.is_anonymous);

        let partial_body = lower_default_declaration(
            "export default function makeValue() {\n  return { ok: true };\n}",
        );
        assert_eq!(
            partial_body.declaration_source.source,
            "function makeValue() {\n  return { ok: true };\n}"
        );
        assert!(!partial_body.is_anonymous);
        assert_eq!(partial_body.local_name.as_deref(), Some("makeValue"));

        let named_class = lower_default_declaration("export default class ValueBox {}");
        assert_eq!(named_class.declaration_source.source, "class ValueBox {}");
        assert!(!named_class.is_anonymous);
        assert_eq!(named_class.local_name.as_deref(), Some("ValueBox"));

        assert!(matches!(
            lower_one_module_statement("export default function named() {}"),
            ModuleAstStatement::ExportDefaultDeclaration(_)
        ));
    }

    #[test]
    fn parses_script_var_and_function_declared_names_with_oxc() {
        let names = parse_script_var_and_function_declared_names(
            r#"
            var { alpha, beta: gamma } = data;
            for (var delta in data) {}
            if (ready) { function boot() {} }
            function outer() { var hidden = 1; function nested() {} }
            "#,
        )
        .expect("script should parse");

        assert_eq!(
            names,
            vec![
                "alpha".to_owned(),
                "boot".to_owned(),
                "delta".to_owned(),
                "gamma".to_owned(),
                "outer".to_owned(),
            ]
        );
    }

    #[test]
    fn script_declared_names_reject_unparseable_source() {
        let error = parse_script_var_and_function_declared_names(
            "var O=function(){},exports={},a=1,function(){return 1},window=2;",
        )
        .expect_err("unparseable source should not produce guessed declarations");

        assert_eq!(error, "unsupported script syntax");
    }

    #[test]
    fn parses_top_level_assignment_declared_names_with_oxc() {
        let names = parse_script_top_level_assignment_declared_names(
            r#"
            onload = function() {
                location = "next.html";
                window.nested = 1;
            };
            window.exported = 1;
            bare = 2;
            ({ destructured } = value);
            "#,
        )
        .expect("script should parse");

        assert_eq!(
            names,
            vec![
                "bare".to_owned(),
                "exported".to_owned(),
                "onload".to_owned(),
            ]
        );
    }

    #[test]
    fn parses_top_level_lexical_declared_names_with_oxc() {
        let names = parse_script_top_level_lexical_declared_names(
            r#"
            const wrapThreshold = 1;
            let { alpha, nested: [beta] } = value;
            if (ready) { const hidden = 2; }
            var ignored = 3;
            "#,
        )
        .expect("script should parse");

        assert_eq!(
            names,
            vec![
                "alpha".to_owned(),
                "beta".to_owned(),
                "wrapThreshold".to_owned(),
            ]
        );
    }

    #[test]
    fn parses_default_declaration_local_names() {
        let function = lower_default_declaration("export default function localName() {}");
        assert_eq!(function.local_name.as_deref(), Some("localName"));
        assert!(!function.is_anonymous);

        let generator = lower_default_declaration("export default async function* localGen() {}");
        assert_eq!(generator.local_name.as_deref(), Some("localGen"));
        assert!(!generator.is_anonymous);

        let class = lower_default_declaration("export default class LocalClass {}");
        assert_eq!(class.local_name.as_deref(), Some("LocalClass"));
        assert!(!class.is_anonymous);

        let anonymous = lower_default_declaration("export default function () {}");
        assert!(anonymous.local_name.is_none());
        assert!(anonymous.is_anonymous);

        let anonymous_class = lower_default_declaration("export default class extends Base {}");
        assert!(anonymous_class.local_name.is_none());
        assert!(anonymous_class.is_anonymous);

        let newline_extends_class =
            lower_default_declaration("export default class extends\nBase {}");
        assert!(newline_extends_class.local_name.is_none());
        assert!(newline_extends_class.is_anonymous);

        let extends_prefix_name = lower_default_declaration("export default class extendsFoo {}");
        assert_eq!(
            extends_prefix_name.local_name.as_deref(),
            Some("extendsFoo")
        );
        assert!(!extends_prefix_name.is_anonymous);
    }
}
