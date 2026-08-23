use super::*;

pub(in crate::context_bootstrap) fn font_face_set_constructor_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    if !args.is_construct_call() {
        throw_type_error(scope, "Constructor must be called with new");
        return;
    }
    if args.length() > 0 {
        throw_type_error(scope, "Illegal constructor");
        return;
    }
    initialize_font_face_set_object(scope, args.this());
    rv.set(args.this().into());
}
