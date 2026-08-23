use super::reactions::enter_custom_element_reaction;

use super::super::native_bridge::JsContextHost;

pub(super) enum CustomElementConstructorInvocation {
    Created(v8::Global<v8::Object>),
    Exception(v8::Global<v8::Value>),
    Empty,
}

pub(super) fn invoke_custom_element_constructor(
    scope: &mut v8::PinScope<'_, '_>,
    host_ptr: *mut JsContextHost,
    constructor: v8::Local<'_, v8::Function>,
) -> CustomElementConstructorInvocation {
    let try_catch = std::pin::pin!(v8::TryCatch::new(scope));
    let scope = try_catch.init();
    let created = {
        let _reaction = enter_custom_element_reaction(host_ptr);
        constructor.new_instance(&scope, &[])
    };
    match created {
        Some(object) => {
            CustomElementConstructorInvocation::Created(v8::Global::new(&scope, object))
        }
        None if scope.has_caught() => scope
            .exception()
            .map(|exception| {
                CustomElementConstructorInvocation::Exception(v8::Global::new(&scope, exception))
            })
            .unwrap_or(CustomElementConstructorInvocation::Empty),
        None => CustomElementConstructorInvocation::Empty,
    }
}
