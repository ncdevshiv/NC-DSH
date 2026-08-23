use super::super::{BridgeHandle, JsContextHost, ReflectorId, runtime_ptr_from_object};

fn object_reflector_id(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) -> std::result::Result<ReflectorId, String> {
    let value = object
        .get_internal_field(scope, 1)
        .ok_or_else(|| "wrapper missing reflector field".to_owned())?;
    let value = v8::Local::<v8::Value>::try_from(value)
        .map_err(|_| "wrapper reflector field had invalid type".to_owned())?;
    let number = value
        .number_value(scope)
        .ok_or_else(|| "wrapper reflector field was not numeric".to_owned())?;
    if !number.is_finite() || number.fract() != 0.0 || number <= 0.0 {
        return Err("wrapper reflector field was invalid".to_owned());
    }
    Ok(ReflectorId::from_raw(number as u64))
}

pub(in crate::native_bridge) fn bridge_handle_from_object(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) -> std::result::Result<(*mut JsContextHost, BridgeHandle), String> {
    let runtime_ptr = runtime_ptr_from_object(scope, object)?;
    let reflector_id = object_reflector_id(scope, object)?;
    let handle = unsafe { &*runtime_ptr }
        .native_bridge()
        .bridge_handle(reflector_id)
        .ok_or_else(|| format!("missing bridge identity `{}`", reflector_id.raw()))?;
    Ok((runtime_ptr, handle))
}
