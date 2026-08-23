pub(crate) fn child_script_declared_global_names(source: &str) -> Vec<String> {
    let mut names = moli_module_syntax::parse_script_var_and_function_declared_names(source)
        .unwrap_or_default();
    names.extend(
        moli_module_syntax::parse_script_top_level_assignment_declared_names(source)
            .unwrap_or_default(),
    );
    names.retain(|name| is_child_script_exportable_binding_name(name));
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
fn child_script_declared_lexical_global_names(source: &str) -> Vec<String> {
    let mut names = moli_module_syntax::parse_script_top_level_lexical_declared_names(source)
        .unwrap_or_default();
    names.retain(|name| is_child_script_exportable_binding_name(name));
    names.sort();
    names.dedup();
    names
}

fn is_child_script_exportable_binding_name(name: &str) -> bool {
    if name.is_empty() || is_js_reserved_word(name) {
        return false;
    }
    if is_child_window_native_binding_name(name) {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

pub(crate) fn is_child_window_native_binding_name(name: &str) -> bool {
    matches!(name, "name")
}

fn is_js_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "let"
            | "static"
    )
}

#[cfg(test)]
mod tests {
    use super::{child_script_declared_global_names, child_script_declared_lexical_global_names};

    #[test]
    fn child_script_global_scan_returns_empty_for_unparseable_sources() {
        let names = child_script_declared_global_names(
            "var O=function(){},exports={},a=1,function(){return 1},window=2;",
        );

        assert!(names.is_empty());
    }

    #[test]
    fn child_script_global_scan_keeps_valid_function_and_var_declarations() {
        let names =
            child_script_declared_global_names("function boot(){} var alpha=1, $beta=2, _gamma=3;");

        assert_eq!(names, vec!["$beta", "_gamma", "alpha", "boot"]);
    }

    #[test]
    fn child_script_global_scan_keeps_simple_global_assignments() {
        let names = child_script_declared_global_names(
            "listener = {};\nhandleEvent = () => {};\nif (ready) { assignedInBlock = 1; }",
        );

        assert_eq!(names, vec!["assignedInBlock", "handleEvent", "listener"]);
    }

    #[test]
    fn child_script_global_scan_ignores_assignments_inside_strings_and_comments() {
        let names = child_script_declared_global_names(
            r#"
            "listener = {}";
            'handleEvent = () => {}';
            `assignedInTemplate = 1`;
            // lineComment = 1
            /* blockComment = 1 */
            realListener = {};
            "#,
        );

        assert_eq!(names, vec!["realListener"]);
    }

    #[test]
    fn child_script_global_scan_keeps_simple_window_assignments() {
        let names = child_script_declared_global_names(
            r#"
            window.exported = 1;
            window.$dollar = 2;
            window._underscore = 3;
            window.notAssigned == 4;
            window.greater >= 5;
            "#,
        );

        assert_eq!(names, vec!["$dollar", "_underscore", "exported"]);
    }

    #[test]
    fn child_script_global_scan_does_not_export_window_name_assignment() {
        let names = child_script_declared_global_names(
            r#"
            window.name = "child";
            name = "bare";
            window.exported = window.name;
            "#,
        );

        assert_eq!(names, vec!["exported"]);
    }

    #[test]
    fn child_script_global_scan_does_not_export_window_property_reads() {
        let names = child_script_declared_global_names(
            r#"
            window.top.sub2_loaded = window.testing == undefined;
            window.top.sub2_count = (window.top.sub2_count || 0) + 1;
            "#,
        );

        assert!(names.is_empty());
    }

    #[test]
    fn child_script_global_scan_ignores_nested_callback_assignments() {
        let names = child_script_declared_global_names(
            r#"
            onload = function() {
                setTimeout(function() { location = "next.html"; }, 100);
                window.nested = 1;
            };
            "#,
        );

        assert_eq!(names, vec!["onload"]);
    }

    #[test]
    fn child_script_global_scan_ignores_window_assignments_inside_strings_and_comments() {
        let names = child_script_declared_global_names(
            r#"
            "window.stringValue = 1";
            `window.templateValue = 2`;
            // window.lineComment = 3
            /* window.blockComment = 4 */
            window.realValue = 5;
            "#,
        );

        assert_eq!(names, vec!["realValue"]);
    }

    #[test]
    fn child_script_global_scan_uses_oxc_for_parseable_script_bindings() {
        let names = child_script_declared_global_names(
            r#"
            var { alpha, beta: gamma } = data;
            for (var delta in data) {}
            if (ready) { function boot() {} }
            function outer() { var hidden = 1; function nested() {} }
            "#,
        );

        assert_eq!(names, vec!["alpha", "boot", "delta", "gamma", "outer"]);
    }

    #[test]
    fn child_script_lexical_global_scan_keeps_top_level_lexical_bindings() {
        let names = child_script_declared_lexical_global_names(
            r#"
            const wrapThreshold = 1;
            let { alpha, nested: [beta] } = value;
            if (ready) { const hidden = 2; }
            var ignored = 3;
            "#,
        );

        assert_eq!(names, vec!["alpha", "beta", "wrapThreshold"]);
    }
}
