use super::*;

pub(super) fn parse_continue_primary_key_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    label: &str,
) -> Option<Key> {
    match parse_idb_key(scope, value) {
        Ok(Some(key)) => Some(key),
        _ => {
            let message =
                format!("Failed to execute 'continuePrimaryKey': the {label} is not valid.");
            let error = dom_exception_value(scope, &message, "DataError");
            scope.throw_exception(error);
            None
        }
    }
}
