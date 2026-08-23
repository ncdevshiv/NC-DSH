use super::helpers::character_data_utf16_units;
use super::*;

pub(in crate::native_bridge) fn node_character_data_length_from_object(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) -> i32 {
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_object(scope, object) else {
        return 0;
    };
    character_data_utf16_units(unsafe { &*runtime_ptr }, handle)
        .map(|data| data.len() as i32)
        .unwrap_or(0)
}
