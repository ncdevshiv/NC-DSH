use oxc_ast::ast::{ImportAttributeKey, ImportDeclaration, ImportDeclarationSpecifier};

use crate::exports::module_export_name;
use crate::types::{ModuleNamedBinding, ParsedModuleStaticImport};

pub(crate) fn parsed_static_import_from_oxc(
    import: &ImportDeclaration<'_>,
) -> Result<ParsedModuleStaticImport, String> {
    let mut default_binding = None;
    let mut namespace_binding = None;
    let mut named_bindings = Vec::new();

    if let Some(specifiers) = &import.specifiers {
        for specifier in specifiers {
            match specifier {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                    default_binding = Some(default.local.name.as_str().to_owned());
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                    namespace_binding = Some(namespace.local.name.as_str().to_owned());
                }
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    named_bindings.push(ModuleNamedBinding {
                        imported_name: module_export_name(&named.imported),
                        local_name: named.local.name.as_str().to_owned(),
                    });
                }
            }
        }
    }

    Ok(ParsedModuleStaticImport {
        specifier: import.source.value.as_str().to_owned(),
        import_type: import_type_from_with_clause(import.with_clause.as_deref())?,
        default_binding,
        namespace_binding,
        named_bindings,
    })
}

pub(crate) fn import_type_from_with_clause(
    with_clause: Option<&oxc_ast::ast::WithClause<'_>>,
) -> Result<Option<String>, String> {
    let Some(with_clause) = with_clause else {
        return Ok(None);
    };
    let mut import_type = None;

    for attribute in &with_clause.with_entries {
        let key = match &attribute.key {
            ImportAttributeKey::Identifier(identifier) => identifier.name.as_str(),
            ImportAttributeKey::StringLiteral(literal) => literal.value.as_str(),
        };
        if key != "type" {
            return Err(format!("unsupported import attribute `{key}`"));
        }
        if import_type.is_some() {
            return Err("unsupported duplicate import attribute `type`".to_owned());
        }
        import_type = Some(attribute.value.value.as_str().to_owned());
    }
    Ok(import_type)
}
