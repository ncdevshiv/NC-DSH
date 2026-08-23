use super::*;

pub(in crate::native_bridge) fn throw_document_domain_security_error(
    scope: &mut v8::PinScope<'_, '_>,
) {
    throw_dom_exception(
        scope,
        "SecurityError",
        18,
        "Failed to set 'domain' on 'Document': the value is not allowed for this document.",
    );
}
