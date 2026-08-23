use super::*;
use moli_webapi_declare::WebApiObject;

#[derive(WebApiObject)]
#[webapi(interface = "MessageChannel")]
struct MessageChannelObjectDeclaration<'scope> {
    #[webapi(slot = MESSAGE_CHANNEL_PORT1_SLOT)]
    port1: v8::Local<'scope, v8::Object>,
    #[webapi(slot = MESSAGE_CHANNEL_PORT2_SLOT)]
    port2: v8::Local<'scope, v8::Object>,
}

pub(in crate::context_bootstrap) fn message_channel_constructor_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !args.is_construct_call() {
        throw_type_error(
            scope,
            "Failed to construct 'MessageChannel': Please use the 'new' operator.",
        );
        return;
    }

    let Some(owner) = current_message_port_owner(scope) else {
        rv.set_undefined();
        return;
    };
    let Some(registry) = current_message_port_registry(scope) else {
        rv.set_undefined();
        return;
    };
    let (port1_id, port2_id) = registry.create_entangled_message_port_pair(owner);

    let Some(port1) = new_message_port_object(scope, port1_id) else {
        rv.set_undefined();
        return;
    };
    let Some(port2) = new_message_port_object(scope, port2_id) else {
        rv.set_undefined();
        return;
    };

    set_message_port_peer(scope, port1, port2);
    set_message_port_peer(scope, port2, port1);
    MessageChannelObjectDeclaration::new(port1, port2)
        .initialize(scope, args.this())
        .expect("MessageChannel declaration should initialize ports");
    rv.set(args.this().into());
}

pub(in crate::context_bootstrap) fn message_port_constructor_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    throw_type_error(
        scope,
        "Failed to construct 'MessagePort': Illegal constructor.",
    );
}
