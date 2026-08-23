use super::*;

pub(super) fn response_text_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(consumption) = begin_body_consumption_promise(scope, &args, &mut rv) else {
        return;
    };
    finish_body_consumption(
        scope,
        &mut rv,
        consumption,
        NetworkBodyConsumptionKind::Text,
    );
}

pub(super) fn response_json_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(consumption) = begin_body_consumption_promise(scope, &args, &mut rv) else {
        return;
    };
    finish_body_consumption(
        scope,
        &mut rv,
        consumption,
        NetworkBodyConsumptionKind::Json,
    );
}
