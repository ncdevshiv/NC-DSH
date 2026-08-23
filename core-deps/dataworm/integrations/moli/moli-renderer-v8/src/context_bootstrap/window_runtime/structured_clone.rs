use super::*;

pub(in crate::context_bootstrap) fn window_structured_clone_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if let Some(value) = structured_clone_value_with_options(scope, args.get(0), args.get(1)) {
        rv.set(value);
    } else {
        rv.set_undefined();
    }
}
