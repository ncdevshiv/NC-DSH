use crate::{
    context_bootstrap, custom_elements,
    util::{call_global_bridge_method, context_host_ptr_from_global_bridge, v8_string},
};

use super::super::super::{detached_native_handle, detached_state_kind};

pub(in crate::native_bridge) fn set_detached_node_text_content<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    object: v8::Local<'a, v8::Object>,
    value: v8::Local<'a, v8::Value>,
) {
    let text = if value.is_null_or_undefined() {
        String::new()
    } else {
        value
            .to_string(scope)
            .map(|value| value.to_rust_string_lossy(scope))
            .unwrap_or_default()
    };
    let kind = detached_state_kind(scope, object);
    if matches!(kind.as_deref(), Some("document" | "doctype")) {
        return;
    }
    if let Some(runtime_ptr) = context_host_ptr_from_global_bridge(scope)
        && let Some(handle) = detached_native_handle(scope, object)
    {
        let removed_count = matches!(
            kind.as_deref(),
            Some("text" | "comment" | "cdataSection" | "processingInstruction")
        )
        .then(|| unsafe { &*runtime_ptr }.character_data_utf16_units(handle))
        .flatten()
        .map(|units| units.len() as u32);
        let inserted_count = text.encode_utf16().count() as u32;
        custom_elements::with_custom_element_reaction_scope(scope, runtime_ptr, |scope| {
            let runtime = unsafe { &mut *runtime_ptr };
            let _ = runtime.set_text_content_appending_to_current_reaction_queue(
                scope,
                runtime_ptr,
                handle,
                &text,
            );
            if let Some(removed_count) = removed_count {
                context_bootstrap::live_ranges_character_data_reset(
                    scope,
                    handle,
                    removed_count,
                    inserted_count,
                );
            }
        });
        return;
    }
    match kind.as_deref() {
        Some("text" | "comment" | "cdataSection" | "processingInstruction") => {
            let text = v8_string(scope, &text)
                .map(Into::<v8::Local<'_, v8::Value>>::into)
                .unwrap_or_else(|| v8::String::empty(scope).into());
            let _ = call_global_bridge_method(
                scope,
                "__setDetachedCharacterData",
                &[object.into(), text],
            );
        }
        _ => {
            let replacement = if text.is_empty() {
                Vec::new()
            } else {
                vec![
                    v8_string(scope, &text)
                        .map(Into::<v8::Local<'_, v8::Value>>::into)
                        .unwrap_or_else(|| v8::String::empty(scope).into()),
                ]
            };
            let mut helper_args = Vec::with_capacity(replacement.len() + 1);
            helper_args.push(object.into());
            helper_args.extend(replacement);
            let _ = call_global_bridge_method(scope, "__detachedReplaceChildren", &helper_args);
        }
    }
}
