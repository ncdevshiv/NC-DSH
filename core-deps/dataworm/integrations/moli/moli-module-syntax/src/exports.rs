use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentTarget, BindingPattern, BlockStatement, Class, Declaration,
    ExportAllDeclaration, ExportDefaultDeclaration, ExportDefaultDeclarationKind,
    ExportNamedDeclaration, Expression, ForStatementInit, ForStatementLeft, Function, FunctionBody,
    ModuleExportName, SimpleAssignmentTarget, Statement, VariableDeclaration,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser as OxcParser;
use oxc_span::{GetSpan, SourceType, Span};

use crate::imports::import_type_from_with_clause;
use crate::types::{
    ModuleExportBinding, ModulePredeclaredExportSurface, ParsedExportConst, ParsedExportVariable,
    ParsedExportVariableBinding, ParsedExportedClass, ParsedExportedFunction,
    ParsedModuleExportAll, ParsedModuleExportList,
};

pub fn parse_script_var_and_function_declared_names(source: &str) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let parsed = OxcParser::new(&allocator, source, SourceType::script()).parse();
    if !parsed.errors.is_empty() {
        return Err("unsupported script syntax".to_owned());
    }

    let mut names = Vec::new();
    for statement in &parsed.program.body {
        collect_script_declared_names_from_statement(statement, &mut names);
    }
    names.sort();
    names.dedup();
    Ok(names)
}

pub fn parse_script_top_level_assignment_declared_names(
    source: &str,
) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let parsed = OxcParser::new(&allocator, source, SourceType::script()).parse();
    if !parsed.errors.is_empty() {
        return Err("unsupported script syntax".to_owned());
    }

    let mut collector = ScriptTopLevelAssignmentNameCollector::default();
    collector.visit_program(&parsed.program);
    collector.names.sort();
    collector.names.dedup();
    Ok(collector.names)
}

pub fn parse_script_top_level_lexical_declared_names(source: &str) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let parsed = OxcParser::new(&allocator, source, SourceType::script()).parse();
    if !parsed.errors.is_empty() {
        return Err("unsupported script syntax".to_owned());
    }

    let mut names = Vec::new();
    for statement in &parsed.program.body {
        if let Statement::VariableDeclaration(declaration) = statement
            && !declaration.kind.is_var()
        {
            for declarator in &declaration.declarations {
                collect_binding_names(&declarator.id, &mut names);
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

#[derive(Default)]
struct ScriptTopLevelAssignmentNameCollector {
    names: Vec<String>,
    function_depth: usize,
}

impl ScriptTopLevelAssignmentNameCollector {
    fn push_assignment_target_name(&mut self, target: &AssignmentTarget<'_>) {
        if self.function_depth != 0 {
            return;
        }
        match target {
            AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
                self.names.push(identifier.name.as_str().to_owned());
            }
            AssignmentTarget::StaticMemberExpression(member)
                if expression_is_window_identifier(&member.object) =>
            {
                self.names.push(member.property.name.as_str().to_owned());
            }
            _ => {}
        }
    }
}

impl<'a> Visit<'a> for ScriptTopLevelAssignmentNameCollector {
    fn visit_assignment_expression(&mut self, expression: &AssignmentExpression<'a>) {
        self.push_assignment_target_name(&expression.left);
        walk::walk_assignment_expression(self, expression);
    }

    fn visit_function_body(&mut self, body: &FunctionBody<'a>) {
        self.function_depth += 1;
        walk::walk_function_body(self, body);
        self.function_depth -= 1;
    }

    fn visit_simple_assignment_target(&mut self, target: &SimpleAssignmentTarget<'a>) {
        if self.function_depth == 0
            && let SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) = target
        {
            self.names.push(identifier.name.as_str().to_owned());
        }
        walk::walk_simple_assignment_target(self, target);
    }
}

fn expression_is_window_identifier(expression: &Expression<'_>) -> bool {
    let Expression::Identifier(identifier) = expression else {
        return false;
    };
    identifier.name == "window"
}

fn collect_script_declared_names_from_statement(
    statement: &Statement<'_>,
    names: &mut Vec<String>,
) {
    match statement {
        Statement::BlockStatement(block) => {
            collect_script_declared_names_from_block(block, names);
        }
        Statement::DoWhileStatement(statement) => {
            collect_script_declared_names_from_statement(&statement.body, names);
        }
        Statement::ForInStatement(statement) => {
            collect_script_declared_names_from_for_left(&statement.left, names);
            collect_script_declared_names_from_statement(&statement.body, names);
        }
        Statement::ForOfStatement(statement) => {
            collect_script_declared_names_from_for_left(&statement.left, names);
            collect_script_declared_names_from_statement(&statement.body, names);
        }
        Statement::ForStatement(statement) => {
            if let Some(ForStatementInit::VariableDeclaration(declaration)) = &statement.init {
                collect_var_declared_names(declaration, names);
            }
            collect_script_declared_names_from_statement(&statement.body, names);
        }
        Statement::IfStatement(statement) => {
            collect_script_declared_names_from_statement(&statement.consequent, names);
            if let Some(alternate) = &statement.alternate {
                collect_script_declared_names_from_statement(alternate, names);
            }
        }
        Statement::LabeledStatement(statement) => {
            collect_script_declared_names_from_statement(&statement.body, names);
        }
        Statement::SwitchStatement(statement) => {
            for case in &statement.cases {
                for statement in &case.consequent {
                    collect_script_declared_names_from_statement(statement, names);
                }
            }
        }
        Statement::TryStatement(statement) => {
            collect_script_declared_names_from_block(&statement.block, names);
            if let Some(handler) = &statement.handler {
                collect_script_declared_names_from_block(&handler.body, names);
            }
            if let Some(finalizer) = &statement.finalizer {
                collect_script_declared_names_from_block(finalizer, names);
            }
        }
        Statement::WhileStatement(statement) => {
            collect_script_declared_names_from_statement(&statement.body, names);
        }
        Statement::WithStatement(statement) => {
            collect_script_declared_names_from_statement(&statement.body, names);
        }
        Statement::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                names.push(id.name.as_str().to_owned());
            }
        }
        Statement::VariableDeclaration(declaration) => {
            collect_var_declared_names(declaration, names);
        }
        _ => {}
    }
}

fn collect_script_declared_names_from_block(block: &BlockStatement<'_>, names: &mut Vec<String>) {
    for statement in &block.body {
        collect_script_declared_names_from_statement(statement, names);
    }
}

fn collect_script_declared_names_from_for_left(
    left: &ForStatementLeft<'_>,
    names: &mut Vec<String>,
) {
    if let ForStatementLeft::VariableDeclaration(declaration) = left {
        collect_var_declared_names(declaration, names);
    }
}

fn collect_var_declared_names(declaration: &VariableDeclaration<'_>, names: &mut Vec<String>) {
    if !declaration.kind.is_var() {
        return;
    }
    for declarator in &declaration.declarations {
        collect_binding_names(&declarator.id, names);
    }
}

fn collect_binding_names(pattern: &BindingPattern<'_>, names: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => {
            names.push(identifier.name.as_str().to_owned());
        }
        BindingPattern::ObjectPattern(pattern) => {
            for property in &pattern.properties {
                collect_binding_names(&property.value, names);
            }
            if let Some(rest) = &pattern.rest {
                collect_binding_names(&rest.argument, names);
            }
        }
        BindingPattern::ArrayPattern(pattern) => {
            for element in pattern.elements.iter().flatten() {
                collect_binding_names(element, names);
            }
            if let Some(rest) = &pattern.rest {
                collect_binding_names(&rest.argument, names);
            }
        }
        BindingPattern::AssignmentPattern(pattern) => {
            collect_binding_names(&pattern.left, names);
        }
    }
}

pub(crate) fn parsed_module_export_list_from_oxc(
    export: &ExportNamedDeclaration<'_>,
) -> Result<ParsedModuleExportList, String> {
    Ok(ParsedModuleExportList {
        specifier: export
            .source
            .as_ref()
            .map(|source| source.value.as_str().to_owned()),
        import_type: import_type_from_with_clause(export.with_clause.as_deref())?,
        bindings: export
            .specifiers
            .iter()
            .map(|specifier| ModuleExportBinding {
                local_name: module_export_name(&specifier.local),
                export_name: module_export_name(&specifier.exported),
            })
            .collect(),
    })
}

pub(crate) fn parsed_module_export_all_from_oxc(
    export: &ExportAllDeclaration<'_>,
) -> Result<ParsedModuleExportAll, String> {
    Ok(ParsedModuleExportAll {
        specifier: export.source.value.as_str().to_owned(),
        import_type: import_type_from_with_clause(export.with_clause.as_deref())?,
        namespace_export_name: export.exported.as_ref().map(module_export_name),
    })
}

fn export_bindings_from_variable_declaration(
    declaration: &VariableDeclaration<'_>,
) -> Vec<ParsedExportVariableBinding> {
    let mut names = Vec::new();
    for declarator in &declaration.declarations {
        collect_binding_pattern_names(&declarator.id, &mut names);
    }
    names
        .into_iter()
        .map(|name| ParsedExportVariableBinding {
            local_name: name.clone(),
            export_name: name,
        })
        .collect()
}

pub(crate) fn parsed_export_const_from_variable_declaration(
    declaration: &VariableDeclaration<'_>,
    source: &str,
) -> std::result::Result<ParsedExportConst, String> {
    parsed_export_const_from_bindings(
        export_bindings_from_variable_declaration(declaration),
        source,
    )
}

fn parsed_export_const_from_bindings(
    bindings: Vec<ParsedExportVariableBinding>,
    source: &str,
) -> std::result::Result<ParsedExportConst, String> {
    if bindings.is_empty() {
        return Err(format!("unsupported export const syntax `{source}`"));
    }
    Ok(ParsedExportConst { bindings })
}

pub(crate) fn parsed_export_variable_from_variable_declaration(
    declaration: &VariableDeclaration<'_>,
    source: &str,
) -> std::result::Result<ParsedExportVariable, String> {
    parsed_export_variable_from_bindings(
        export_bindings_from_variable_declaration(declaration),
        source,
    )
}

fn parsed_export_variable_from_bindings(
    bindings: Vec<ParsedExportVariableBinding>,
    source: &str,
) -> std::result::Result<ParsedExportVariable, String> {
    if bindings.is_empty() {
        return Err(format!("unsupported export variable syntax `{source}`"));
    }
    Ok(ParsedExportVariable { bindings })
}

pub(crate) fn parsed_exported_function_from_oxc(
    function: &Function<'_>,
    source: &str,
) -> std::result::Result<ParsedExportedFunction, String> {
    let Some(name) = function.id.as_ref() else {
        return Err(format!("unsupported exported function syntax `{source}`"));
    };
    let name = name.name.as_str().to_owned();
    Ok(ParsedExportedFunction {
        local_name: name.clone(),
        export_name: name,
    })
}

pub(crate) fn parsed_exported_class_from_oxc(
    class: &Class<'_>,
    source: &str,
) -> std::result::Result<ParsedExportedClass, String> {
    let Some(name) = class.id.as_ref() else {
        return Err(format!("unsupported exported class syntax `{source}`"));
    };
    let name = name.name.as_str().to_owned();
    Ok(ParsedExportedClass {
        local_name: name.clone(),
        export_name: name,
    })
}

pub(crate) fn export_default_expression_span_from_oxc(
    source: &str,
    statement_source: &str,
    declaration: &ExportDefaultDeclarationKind<'_>,
) -> std::result::Result<Span, String> {
    match declaration {
        ExportDefaultDeclarationKind::FunctionDeclaration(_)
        | ExportDefaultDeclarationKind::ClassDeclaration(_)
        | ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => {
            Err(unsupported_export_default_syntax(statement_source))
        }
        declaration => source_for_span(source, declaration.span())
            .filter(|expression| !expression.trim().is_empty())
            .map(|_| declaration.span())
            .ok_or_else(|| unsupported_export_default_syntax(statement_source)),
    }
}

pub(crate) struct ParsedExportDefaultDeclarationParts {
    pub declaration_span: Span,
    pub is_anonymous: bool,
    pub local_name: Option<String>,
}

pub(crate) fn parsed_export_default_declaration_parts_from_oxc(
    source: &str,
    statement_source: &str,
    export: &ExportDefaultDeclaration<'_>,
) -> std::result::Result<ParsedExportDefaultDeclarationParts, String> {
    let (is_anonymous, local_name, declaration_span) = match &export.declaration {
        ExportDefaultDeclarationKind::FunctionDeclaration(function) => (
            function.id.is_none(),
            function.id.as_ref().map(|id| id.name.as_str().to_owned()),
            function.span(),
        ),
        ExportDefaultDeclarationKind::ClassDeclaration(class) => (
            class.id.is_none(),
            class.id.as_ref().map(|id| id.name.as_str().to_owned()),
            class.span(),
        ),
        _ => return Err(unsupported_export_default_syntax(statement_source)),
    };
    source_for_span(source, declaration_span)
        .filter(|declaration| !declaration.trim().is_empty())
        .ok_or_else(|| unsupported_export_default_syntax(statement_source))?;
    Ok(ParsedExportDefaultDeclarationParts {
        declaration_span,
        is_anonymous,
        local_name,
    })
}

fn unsupported_export_default_syntax(source: &str) -> String {
    format!(
        "unsupported export default syntax `{}`",
        concise_source_context(source)
    )
}

fn concise_source_context(source: &str) -> String {
    const MAX_CHARS: usize = 160;
    let trimmed = source.trim();
    let mut chars = trimmed.chars();
    let snippet: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{snippet}...")
    } else {
        snippet
    }
}

#[cfg(test)]
pub(crate) fn collect_predeclared_export_surface(
    source: &str,
) -> std::result::Result<ModulePredeclaredExportSurface, String> {
    collect_predeclared_export_surface_with_oxc(source)
}

#[cfg(test)]
fn collect_predeclared_export_surface_with_oxc(
    source: &str,
) -> std::result::Result<ModulePredeclaredExportSurface, String> {
    let allocator = Allocator::default();
    let parsed = OxcParser::new(&allocator, source, SourceType::mjs()).parse();
    if let Some(error) = parsed.errors.first() {
        return Err(format!("failed to parse module source with oxc: {error:?}"));
    }

    Ok(collect_predeclared_export_surface_from_statements(
        &parsed.program.body,
    ))
}

pub(crate) fn collect_predeclared_export_surface_from_statements(
    statements: &[Statement<'_>],
) -> ModulePredeclaredExportSurface {
    let mut surface = ModulePredeclaredExportSurface::default();
    let mut seen = HashSet::new();
    for statement in statements {
        collect_statement_export_surface(statement, &mut surface, &mut seen);
    }
    surface
}

fn collect_statement_export_surface(
    statement: &Statement<'_>,
    surface: &mut ModulePredeclaredExportSurface,
    seen: &mut HashSet<String>,
) {
    match statement {
        Statement::ExportDefaultDeclaration(_) => {
            push_predeclared_export_name(&mut surface.explicit_export_names, seen, "default");
        }
        Statement::ExportNamedDeclaration(export) => {
            collect_named_export_surface(export, surface, seen);
        }
        Statement::ExportAllDeclaration(export) => {
            collect_export_all_surface(export, surface, seen);
        }
        _ => {}
    }
}

fn collect_named_export_surface(
    export: &ExportNamedDeclaration<'_>,
    surface: &mut ModulePredeclaredExportSurface,
    seen: &mut HashSet<String>,
) {
    if let Some(declaration) = &export.declaration {
        collect_declaration_export_surface(declaration, surface, seen);
    }
    for specifier in &export.specifiers {
        push_predeclared_export_name(
            &mut surface.explicit_export_names,
            seen,
            &module_export_name(&specifier.exported),
        );
    }
}

fn collect_export_all_surface(
    export: &ExportAllDeclaration<'_>,
    surface: &mut ModulePredeclaredExportSurface,
    seen: &mut HashSet<String>,
) {
    if let Some(exported) = &export.exported {
        push_predeclared_export_name(
            &mut surface.explicit_export_names,
            seen,
            &module_export_name(exported),
        );
    } else {
        surface.has_export_star = true;
    }
}

fn collect_declaration_export_surface(
    declaration: &Declaration<'_>,
    surface: &mut ModulePredeclaredExportSurface,
    seen: &mut HashSet<String>,
) {
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            collect_variable_declaration_export_surface(declaration, surface, seen);
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(name) = function.id.as_ref() {
                push_predeclared_export_name(
                    &mut surface.explicit_export_names,
                    seen,
                    name.name.as_str(),
                );
            }
        }
        Declaration::ClassDeclaration(class) => {
            collect_class_export_surface(class, surface, seen);
        }
        _ => {}
    }
}

fn collect_variable_declaration_export_surface(
    declaration: &VariableDeclaration<'_>,
    surface: &mut ModulePredeclaredExportSurface,
    seen: &mut HashSet<String>,
) {
    for declarator in &declaration.declarations {
        collect_binding_pattern_export_names(
            &declarator.id,
            &mut surface.explicit_export_names,
            seen,
        );
    }
}

fn collect_class_export_surface(
    class: &Class<'_>,
    surface: &mut ModulePredeclaredExportSurface,
    seen: &mut HashSet<String>,
) {
    if let Some(name) = class.id.as_ref() {
        push_predeclared_export_name(&mut surface.explicit_export_names, seen, name.name.as_str());
    }
}

fn collect_binding_pattern_export_names(
    pattern: &BindingPattern<'_>,
    export_names: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => {
            push_predeclared_export_name(export_names, seen, identifier.name.as_str());
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                collect_binding_pattern_export_names(&property.value, export_names, seen);
            }
            if let Some(rest) = object.rest.as_ref() {
                collect_binding_pattern_export_names(&rest.argument, export_names, seen);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                collect_binding_pattern_export_names(element, export_names, seen);
            }
            if let Some(rest) = array.rest.as_ref() {
                collect_binding_pattern_export_names(&rest.argument, export_names, seen);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => {
            collect_binding_pattern_export_names(&assignment.left, export_names, seen);
        }
    }
}

fn collect_binding_pattern_names(pattern: &BindingPattern<'_>, names: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => {
            names.push(identifier.name.as_str().to_owned());
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                collect_binding_pattern_names(&property.value, names);
            }
            if let Some(rest) = object.rest.as_ref() {
                collect_binding_pattern_names(&rest.argument, names);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                collect_binding_pattern_names(element, names);
            }
            if let Some(rest) = array.rest.as_ref() {
                collect_binding_pattern_names(&rest.argument, names);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => {
            collect_binding_pattern_names(&assignment.left, names);
        }
    }
}

fn source_for_span(source: &str, span: Span) -> Option<&str> {
    source.get(span.start as usize..span.end as usize)
}

pub(crate) fn module_export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(identifier) => identifier.name.as_str().to_owned(),
        ModuleExportName::IdentifierReference(identifier) => identifier.name.as_str().to_owned(),
        ModuleExportName::StringLiteral(literal) => literal.value.as_str().to_owned(),
    }
}

fn push_predeclared_export_name(
    export_names: &mut Vec<String>,
    seen: &mut HashSet<String>,
    export_name: &str,
) {
    if seen.insert(export_name.to_owned()) {
        export_names.push(export_name.to_owned());
    }
}
