use oxc_allocator::Allocator;
use oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind, Statement, VariableDeclarationKind};
use oxc_parser::Parser as OxcParser;
use oxc_span::{GetSpan, SourceType, Span};

use crate::dynamic_import::collect_module_rewrite_sites_from_program;
use crate::exports::{
    collect_predeclared_export_surface_from_statements, export_default_expression_span_from_oxc,
    parsed_export_const_from_variable_declaration,
    parsed_export_default_declaration_parts_from_oxc,
    parsed_export_variable_from_variable_declaration, parsed_exported_class_from_oxc,
    parsed_exported_function_from_oxc, parsed_module_export_all_from_oxc,
    parsed_module_export_list_from_oxc,
};
use crate::imports::parsed_static_import_from_oxc;
use crate::types::{
    DynamicImportRewriteSite, ImportMetaResolveRewriteSite, ModuleAstLowering,
    ModuleAstSourceFragment, ModuleAstSpan, ModuleAstStatement, ModuleSpecialFormRewriteSites,
    ParsedExportDefaultDeclaration,
};

#[cfg(test)]
pub(crate) fn lower_module_source_with_ast(
    source: &str,
) -> Result<Vec<ModuleAstStatement>, String> {
    Ok(lower_module_source_with_ast_lowering(source)?.statements)
}

pub fn lower_module_source_with_ast_lowering(source: &str) -> Result<ModuleAstLowering, String> {
    let allocator = Allocator::default();
    let parsed = OxcParser::new(&allocator, source, SourceType::mjs()).parse();
    if !parsed.errors.is_empty() {
        return Err("unsupported module syntax while lowering AST statements".to_owned());
    }

    let rewrite_sites = collect_module_rewrite_sites_from_program(source, &parsed.program)?;
    let predeclared_export_surface =
        collect_predeclared_export_surface_from_statements(&parsed.program.body);
    let mut statements = Vec::new();
    let mut cursor = parsed
        .program
        .hashbang
        .as_ref()
        .map_or(0usize, |hashbang| hashbang.span.end as usize);

    for statement in &parsed.program.body {
        let statement_span = statement.span();
        let start = statement_span.start as usize;
        let end = statement_span.end as usize;
        if cursor < start {
            push_non_whitespace_body_chunk(
                &mut statements,
                source,
                cursor,
                start,
                &rewrite_sites.special_forms,
                &rewrite_sites.top_level_awaits,
            )?;
        }
        statements.push(lower_ast_statement(
            source,
            statement,
            &rewrite_sites.special_forms,
            &rewrite_sites.top_level_awaits,
        )?);
        cursor = end;
    }

    if cursor < source.len() {
        push_non_whitespace_body_chunk(
            &mut statements,
            source,
            cursor,
            source.len(),
            &rewrite_sites.special_forms,
            &rewrite_sites.top_level_awaits,
        )?;
    }
    if statements.is_empty() {
        statements.push(ModuleAstStatement::Empty);
    }

    Ok(ModuleAstLowering {
        predeclared_export_surface,
        statements,
    })
}

fn push_non_whitespace_body_chunk(
    statements: &mut Vec<ModuleAstStatement>,
    source: &str,
    start: usize,
    end: usize,
    special_forms: &ModuleSpecialFormRewriteSites,
    top_level_awaits: &[ModuleAstSpan],
) -> Result<(), String> {
    let chunk = source
        .get(start..end)
        .ok_or_else(|| "failed to slice module source gap with AST span".to_owned())?;
    if !chunk.trim().is_empty() {
        statements.push(ModuleAstStatement::Body(source_fragment_for_range(
            source,
            start,
            end,
            special_forms,
            top_level_awaits,
        )?));
    }
    Ok(())
}

fn lower_ast_statement(
    source: &str,
    statement: &Statement<'_>,
    special_forms: &ModuleSpecialFormRewriteSites,
    top_level_awaits: &[ModuleAstSpan],
) -> Result<ModuleAstStatement, String> {
    let statement_source = source_for_span(source, statement.span())?;
    match statement {
        Statement::ImportDeclaration(import) => {
            parsed_static_import_from_oxc(import).map(ModuleAstStatement::StaticImport)
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(declaration) = export.declaration.as_ref() {
                return lower_exported_declaration(
                    source,
                    statement_source,
                    statement.span(),
                    declaration,
                    special_forms,
                    top_level_awaits,
                );
            }
            parsed_module_export_list_from_oxc(export).map(ModuleAstStatement::ExportList)
        }
        Statement::ExportAllDeclaration(export) => {
            parsed_module_export_all_from_oxc(export).map(ModuleAstStatement::ExportAll)
        }
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(_)
            | ExportDefaultDeclarationKind::ClassDeclaration(_) => {
                let parts = parsed_export_default_declaration_parts_from_oxc(
                    source,
                    statement_source,
                    export,
                )?;
                Ok(ModuleAstStatement::ExportDefaultDeclaration(
                    ParsedExportDefaultDeclaration {
                        declaration_source: source_fragment_for_span(
                            source,
                            parts.declaration_span,
                            special_forms,
                            top_level_awaits,
                        )?,
                        is_anonymous: parts.is_anonymous,
                        local_name: parts.local_name,
                    },
                ))
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => Err(format!(
                "unsupported export default declaration syntax `{statement_source}`"
            )),
            declaration => {
                let expression_span =
                    export_default_expression_span_from_oxc(source, statement_source, declaration)?;
                source_fragment_for_span(source, expression_span, special_forms, top_level_awaits)
                    .map(ModuleAstStatement::ExportDefaultExpr)
            }
        },
        _ => source_fragment_for_span(source, statement.span(), special_forms, top_level_awaits)
            .map(ModuleAstStatement::Body),
    }
}

fn lower_exported_declaration(
    source: &str,
    statement_source: &str,
    statement_span: Span,
    declaration: &Declaration<'_>,
    special_forms: &ModuleSpecialFormRewriteSites,
    top_level_awaits: &[ModuleAstSpan],
) -> Result<ModuleAstStatement, String> {
    let local_source = source_fragment_for_range(
        source,
        declaration.span().start as usize,
        statement_span.end as usize,
        special_forms,
        top_level_awaits,
    )?;
    match declaration {
        Declaration::FunctionDeclaration(function) => {
            parsed_exported_function_from_oxc(function, statement_source).map(|export| {
                ModuleAstStatement::ExportedFunction {
                    export,
                    local_source,
                }
            })
        }
        Declaration::ClassDeclaration(class) => {
            parsed_exported_class_from_oxc(class, statement_source).map(|export| {
                ModuleAstStatement::ExportedClass {
                    export,
                    local_source,
                }
            })
        }
        Declaration::VariableDeclaration(declaration) => {
            match declaration.kind {
                VariableDeclarationKind::Const => {
                    parsed_export_const_from_variable_declaration(declaration, statement_source)
                        .map(|export| ModuleAstStatement::ExportConst {
                            export,
                            local_source,
                        })
                }
                VariableDeclarationKind::Let | VariableDeclarationKind::Var => {
                    parsed_export_variable_from_variable_declaration(declaration, statement_source)
                        .map(|export| ModuleAstStatement::ExportVariable {
                            export,
                            local_source,
                        })
                }
                _ => Err(format!(
                    "unsupported export variable declaration syntax `{statement_source}`"
                )),
            }
        }
        _ => Err(format!(
            "unsupported exported declaration syntax `{statement_source}`"
        )),
    }
}

fn source_for_span(source: &str, span: Span) -> Result<&str, String> {
    source_for_range(source, span.start, span.end)
}

fn source_for_range(source: &str, start: u32, end: u32) -> Result<&str, String> {
    source
        .get(start as usize..end as usize)
        .ok_or_else(|| "failed to slice module source with AST span".to_owned())
}

fn source_fragment_for_span(
    source: &str,
    span: Span,
    special_forms: &ModuleSpecialFormRewriteSites,
    top_level_awaits: &[ModuleAstSpan],
) -> Result<ModuleAstSourceFragment, String> {
    source_fragment_for_range(
        source,
        span.start as usize,
        span.end as usize,
        special_forms,
        top_level_awaits,
    )
}

fn source_fragment_for_range(
    source: &str,
    start: usize,
    end: usize,
    special_forms: &ModuleSpecialFormRewriteSites,
    top_level_awaits: &[ModuleAstSpan],
) -> Result<ModuleAstSourceFragment, String> {
    let fragment_source = source
        .get(start..end)
        .ok_or_else(|| "failed to slice module source with AST span".to_owned())?
        .to_owned();
    Ok(ModuleAstSourceFragment {
        source: fragment_source,
        span: ModuleAstSpan {
            start: start
                .try_into()
                .map_err(|_| "module AST fragment start exceeded u32".to_owned())?,
            end: end
                .try_into()
                .map_err(|_| "module AST fragment end exceeded u32".to_owned())?,
        },
        special_forms: relative_special_forms_for_range(special_forms, start, end)?,
        contains_top_level_await: contains_top_level_await_for_range(top_level_awaits, start, end)?,
    })
}

fn contains_top_level_await_for_range(
    top_level_awaits: &[ModuleAstSpan],
    fragment_start: usize,
    fragment_end: usize,
) -> Result<bool, String> {
    for span in top_level_awaits {
        let await_start = span.start as usize;
        let await_end = span.end as usize;
        if await_end <= fragment_start || await_start >= fragment_end {
            continue;
        }
        if await_start < fragment_start || await_end > fragment_end {
            return Err("module top-level await site crossed AST fragment boundary".to_owned());
        }
        return Ok(true);
    }
    Ok(false)
}

fn relative_special_forms_for_range(
    special_forms: &ModuleSpecialFormRewriteSites,
    fragment_start: usize,
    fragment_end: usize,
) -> Result<ModuleSpecialFormRewriteSites, String> {
    let mut relative = ModuleSpecialFormRewriteSites::default();

    for span in &special_forms.import_metas {
        if let Some((start, end)) = relative_range(
            span.start as usize,
            span.end as usize,
            fragment_start,
            fragment_end,
        )? {
            relative.import_metas.push(ModuleAstSpan {
                start: start
                    .try_into()
                    .map_err(|_| "relative import.meta start exceeded u32".to_owned())?,
                end: end
                    .try_into()
                    .map_err(|_| "relative import.meta end exceeded u32".to_owned())?,
            });
        }
    }

    for span in &special_forms.import_meta_urls {
        if let Some((start, end)) = relative_range(
            span.start as usize,
            span.end as usize,
            fragment_start,
            fragment_end,
        )? {
            relative.import_meta_urls.push(ModuleAstSpan {
                start: start
                    .try_into()
                    .map_err(|_| "relative import.meta.url start exceeded u32".to_owned())?,
                end: end
                    .try_into()
                    .map_err(|_| "relative import.meta.url end exceeded u32".to_owned())?,
            });
        }
    }

    for site in &special_forms.import_meta_resolves {
        if let Some((replace_start, replace_end)) = relative_range(
            site.replace_start,
            site.replace_end,
            fragment_start,
            fragment_end,
        )? {
            relative
                .import_meta_resolves
                .push(ImportMetaResolveRewriteSite {
                    specifier: site.specifier.clone(),
                    replace_start,
                    replace_end,
                });
        }
    }

    for site in &special_forms.dynamic_imports {
        if let Some((replace_start, replace_end)) = relative_range(
            site.replace_start,
            site.replace_end,
            fragment_start,
            fragment_end,
        )? {
            relative.dynamic_imports.push(DynamicImportRewriteSite {
                specifier: site.specifier.clone(),
                resolve_import_meta_first: site.resolve_import_meta_first,
                import_type: site.import_type.clone(),
                replace_start,
                replace_end,
                kind: site.kind,
            });
        }
    }

    Ok(relative)
}

fn relative_range(
    site_start: usize,
    site_end: usize,
    fragment_start: usize,
    fragment_end: usize,
) -> Result<Option<(usize, usize)>, String> {
    if site_end <= fragment_start || site_start >= fragment_end {
        return Ok(None);
    }
    if site_start < fragment_start || site_end > fragment_end {
        return Err("module special-form rewrite site crossed AST fragment boundary".to_owned());
    }
    Ok(Some((
        site_start - fragment_start,
        site_end - fragment_start,
    )))
}

#[cfg(test)]
mod tests {
    use super::lower_module_source_with_ast;
    use crate::types::ModuleAstStatement;

    #[test]
    fn lowers_module_source_with_ast_statement_kinds() {
        let source = [
            "/*! license */",
            "import value from './dep.js';",
            "export const value = 1;",
            "export{value as renamed};",
            "export default value;",
        ]
        .join("\n");

        let statements = lower_module_source_with_ast(&source).expect("module source should lower");

        assert!(matches!(statements[0], ModuleAstStatement::Body(_)));
        assert!(matches!(statements[1], ModuleAstStatement::StaticImport(_)));
        assert!(matches!(
            statements[2],
            ModuleAstStatement::ExportConst { .. }
        ));
        assert!(matches!(statements[3], ModuleAstStatement::ExportList(_)));
        assert!(matches!(
            statements[4],
            ModuleAstStatement::ExportDefaultExpr(_)
        ));
    }
}
