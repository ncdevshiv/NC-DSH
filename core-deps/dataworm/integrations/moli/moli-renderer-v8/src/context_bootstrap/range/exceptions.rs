pub(in crate::context_bootstrap) fn throw_named_dom_exception(
    scope: &mut v8::PinScope<'_, '_>,
    name: &'static str,
    message: &str,
) {
    crate::context_bootstrap::throw_dom_exception_value(scope, message, name);
}
