use super::*;
use crate::util::enqueue_host_microtask;

pub(in crate::context_bootstrap) fn schedule_scope_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    data: Option<v8::Local<'s, v8::Value>>,
) {
    let mut builder = v8::Function::builder(callback);
    if let Some(data) = data {
        builder = builder.data(data);
    }
    let Some(callback) = builder.build(scope) else {
        return;
    };
    enqueue_host_microtask(scope, callback);
}

pub(in crate::context_bootstrap) fn schedule_host_callback(
    scope: &mut v8::PinScope<'_, '_>,
    host: &mut JsContextHost,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let _ = host;
    schedule_scope_callback(scope, callback, None);
}
