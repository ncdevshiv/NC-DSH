use crate::native_bridge::node::{
    dom_string_value_or_throw, require_argument_count, utf16_count_value,
    utf16_index_value_or_throw,
};
use crate::util::{
    string_from_utf16_units_lossy, utf16_len, utf16_replace_units_range_lossy,
    utf16_split_units_lossy, utf16_units,
};
use crate::webidl;

use super::*;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CharacterData.data")]
struct DetachedCharacterDataSetterArgs {
    #[webidl(index = 1, required)]
    value: String,
}

pub(in crate::native_bridge) fn detached_character_data_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
) -> String {
    if detached_has_native_handle(scope, object) {
        return read_detached_native_text_content(scope, object).unwrap_or_default();
    }
    if detached_is_node(scope, object) {
        return detached_state_string(scope, object, "data").unwrap_or_default();
    }
    object_string_property(scope, object, "data")
        .or_else(|| object_string_property(scope, object, "nodeValue"))
        .unwrap_or_default()
}

pub(in crate::native_bridge) fn set_detached_character_data_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    value: &str,
) {
    let Some(state) = detached_state_object(scope, object) else {
        return;
    };
    if let Some(changed) = write_detached_native_text_content(scope, object, value) {
        if changed {
            detached_record_tree_mutation(scope, object);
        }
        return;
    }
    let Some(v8_value) = v8_string(scope, value) else {
        return;
    };
    let _ = state.set(scope, v8str(scope, "data").into(), v8_value.into());
    detached_record_tree_mutation(scope, object);
}

pub(in crate::native_bridge) fn detached_character_data_length<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
) -> i32 {
    utf16_len(&detached_character_data_value(scope, object)) as i32
}

pub(in crate::native_bridge) fn detached_text_whole_text_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
) -> Option<String> {
    let kind = detached_state_kind(scope, object)?;
    matches!(kind.as_str(), "text" | "cdataSection")
        .then(|| detached_character_data_value(scope, object))
}

pub(in crate::native_bridge) fn bridge_detached_character_data_getter_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(node) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        rv.set_empty_string();
        return;
    };
    let data = detached_character_data_value(scope, node);
    set_string_return_value(scope, &mut rv, &data);
}

pub(in crate::native_bridge) fn bridge_detached_character_data_setter_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(node) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        return;
    };
    let Some(parsed) = webidl::parse_args::<DetachedCharacterDataSetterArgs>(scope, &args) else {
        return;
    };
    let removed_count = detached_character_data_length(scope, node) as u32;
    let inserted_count = utf16_len(&parsed.value) as u32;
    set_detached_character_data_value(scope, node, &parsed.value);
    crate::context_bootstrap::live_ranges_detached_character_data_reset(
        scope,
        node,
        removed_count,
        inserted_count,
    );
}

pub(in crate::native_bridge) fn detached_character_data_append_data_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_argument_count(scope, &args, "CharacterData", "appendData", 1) {
        return;
    }
    let this = args.this();
    if detached_state_object(scope, this).is_none() {
        return;
    }
    let mut data = detached_character_data_value(scope, this);
    let Some(arg) = dom_string_value_or_throw(
        scope,
        args.get(0),
        webidl::Context::argument("CharacterData", 1),
    ) else {
        return;
    };
    data.push_str(&arg);
    set_detached_character_data_value(scope, this, &data);
}

pub(in crate::native_bridge) fn detached_character_data_delete_data_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_argument_count(scope, &args, "CharacterData", "deleteData", 2) {
        return;
    }
    let this = args.this();
    if detached_state_object(scope, this).is_none() {
        return;
    }
    let data = detached_character_data_value(scope, this);
    let chars = utf16_units(&data);
    let Some(offset) = utf16_index_value_or_throw(
        scope,
        args.get(0),
        chars.len(),
        webidl::Context::argument("CharacterData", 1),
    ) else {
        return;
    };
    let Some(count) = utf16_count_value(
        scope,
        args.get(1),
        webidl::Context::argument("CharacterData", 2),
    ) else {
        return;
    };
    let end = offset.saturating_add(count).min(chars.len());
    let new_data = utf16_replace_units_range_lossy(&chars, offset, count, &[]);
    set_detached_character_data_value(scope, this, &new_data);
    crate::context_bootstrap::live_ranges_detached_character_data_edit(
        scope,
        this,
        offset as u32,
        (end - offset) as u32,
        0,
    );
}

pub(in crate::native_bridge) fn detached_character_data_insert_data_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_argument_count(scope, &args, "CharacterData", "insertData", 2) {
        return;
    }
    let this = args.this();
    if detached_state_object(scope, this).is_none() {
        return;
    }
    let data = detached_character_data_value(scope, this);
    let chars = utf16_units(&data);
    let Some(offset) = utf16_index_value_or_throw(
        scope,
        args.get(0),
        chars.len(),
        webidl::Context::argument("CharacterData", 1),
    ) else {
        return;
    };
    let Some(arg) = dom_string_value_or_throw(
        scope,
        args.get(1),
        webidl::Context::argument("CharacterData", 2),
    ) else {
        return;
    };
    let arg_units = utf16_units(&arg);
    let new_data = utf16_replace_units_range_lossy(&chars, offset, 0, &arg_units);
    set_detached_character_data_value(scope, this, &new_data);
    crate::context_bootstrap::live_ranges_detached_character_data_edit(
        scope,
        this,
        offset as u32,
        0,
        arg_units.len() as u32,
    );
}

pub(in crate::native_bridge) fn detached_character_data_replace_data_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_argument_count(scope, &args, "CharacterData", "replaceData", 3) {
        return;
    }
    let this = args.this();
    if detached_state_object(scope, this).is_none() {
        return;
    }
    let data = detached_character_data_value(scope, this);
    let chars = utf16_units(&data);
    let Some(offset) = utf16_index_value_or_throw(
        scope,
        args.get(0),
        chars.len(),
        webidl::Context::argument("CharacterData", 1),
    ) else {
        return;
    };
    let Some(count) = utf16_count_value(
        scope,
        args.get(1),
        webidl::Context::argument("CharacterData", 2),
    ) else {
        return;
    };
    let Some(arg) = dom_string_value_or_throw(
        scope,
        args.get(2),
        webidl::Context::argument("CharacterData", 3),
    ) else {
        return;
    };
    let end = offset.saturating_add(count).min(chars.len());
    let arg_units = utf16_units(&arg);
    let new_data = utf16_replace_units_range_lossy(&chars, offset, count, &arg_units);
    set_detached_character_data_value(scope, this, &new_data);
    crate::context_bootstrap::live_ranges_detached_character_data_edit(
        scope,
        this,
        offset as u32,
        (end - offset) as u32,
        arg_units.len() as u32,
    );
}

pub(in crate::native_bridge) fn detached_character_data_substring_data_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_argument_count(scope, &args, "CharacterData", "substringData", 2) {
        return;
    }
    let this = args.this();
    let data = detached_character_data_value(scope, this);
    let chars = utf16_units(&data);
    let Some(offset) = utf16_index_value_or_throw(
        scope,
        args.get(0),
        chars.len(),
        webidl::Context::argument("CharacterData", 1),
    ) else {
        return;
    };
    let Some(count) = utf16_count_value(
        scope,
        args.get(1),
        webidl::Context::argument("CharacterData", 2),
    ) else {
        return;
    };
    let end = offset.saturating_add(count).min(chars.len());
    let result = string_from_utf16_units_lossy(&chars[offset..end]);
    set_string_return_value(scope, &mut rv, &result);
}

pub(in crate::native_bridge) fn detached_text_split_text_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let this = args.this();
    let kind = detached_state_kind(scope, this);
    if !matches!(kind.as_deref(), Some("text" | "cdataSection")) {
        throw_type_error(scope, "Text.splitText requires a Text node");
        return;
    }
    if !require_argument_count(scope, &args, "Text", "splitText", 1) {
        return;
    }
    if detached_state_object(scope, this).is_none() {
        return;
    }
    let data = detached_character_data_value(scope, this);
    let units = utf16_units(&data);
    let Some(offset) = utf16_index_value_or_throw(
        scope,
        args.get(0),
        units.len(),
        webidl::Context::argument("Text", 1),
    ) else {
        return;
    };
    let (before, after) = utf16_split_units_lossy(&units, offset);
    set_detached_character_data_value(scope, this, &before);
    let Some(owner_document) = detached_owner_document_object(scope, this) else {
        rv.set_undefined();
        return;
    };
    let split = match kind.as_deref() {
        Some("cdataSection") => build_detached_cdata_section_object(scope, owner_document, &after),
        _ => build_detached_text_object(scope, owner_document, &after),
    };
    let Some(split) = split else {
        rv.set_undefined();
        return;
    };
    if let Some(parent) = detached_parent_node_object(scope, this) {
        let reference = detached_sibling_object(scope, this, 1);
        if detached_insert_node(scope, parent, split, reference).is_err() {
            rv.set_undefined();
            return;
        }
        crate::context_bootstrap::live_ranges_detached_text_split(
            scope,
            this,
            split,
            offset as u32,
        );
    } else {
        crate::context_bootstrap::live_ranges_detached_character_data_edit(
            scope,
            this,
            offset as u32,
            units.len().saturating_sub(offset) as u32,
            0,
        );
    }
    rv.set(split.into());
}
