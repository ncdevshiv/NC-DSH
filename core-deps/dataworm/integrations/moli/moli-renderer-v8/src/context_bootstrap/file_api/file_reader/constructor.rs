use super::*;

pub(in crate::context_bootstrap) fn file_reader_constructor_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !args.is_construct_call() {
        throw_type_error(
            scope,
            "Failed to construct 'FileReader': Please use the 'new' operator.",
        );
        return;
    }
    initialize_file_reader_object(scope, args.this());
    rv.set(args.this().into());
}
