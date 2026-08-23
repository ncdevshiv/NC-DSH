use super::*;
use crate::webidl;

pub(in crate::context_bootstrap) fn callback_arg_node_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    index: i32,
) -> Option<v8::Local<'s, v8::Object>> {
    let object = v8::Local::<v8::Object>::try_from(args.get(index)).ok()?;
    let node_type = object_number_property(scope, object, "nodeType")?;
    (node_type > 0.0).then_some(object)
}

pub(in crate::context_bootstrap) fn webidl_node_arg<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    index: i32,
    message: &'static str,
) -> Result<v8::Local<'s, v8::Object>, webidl::WebIdlError> {
    callback_arg_node_object(scope, args, index)
        .ok_or_else(|| webidl::WebIdlError::custom_message(message))
}
