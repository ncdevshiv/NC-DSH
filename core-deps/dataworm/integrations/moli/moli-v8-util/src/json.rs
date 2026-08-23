use serde_json::Value as JsonValue;
use v8::{Array, Function, Local, Object, PinScope, Value};

use crate::properties::get_static_property;
use crate::strings::v8_string;

/// Decodes one V8 Inspector protocol message without first materializing an
/// additional UTF-8 `String` when V8 already exposes ASCII JSON bytes.
///
/// Inspector's 8-bit string view contains Latin-1 code units rather than an
/// unconditional UTF-8 byte string. JSON emitted by V8 is normally ASCII at
/// this boundary (non-ASCII string contents are escaped), so that common path
/// can be parsed in place. The non-ASCII 8-bit and 16-bit paths retain the
/// prior text conversion semantics.
pub fn decode_inspector_protocol_message(
    view: v8::inspector::StringView<'_>,
) -> serde_json::Result<JsonValue> {
    if let Some(bytes) = view.characters8() {
        if bytes.is_ascii() {
            return serde_json::from_slice(bytes);
        }
        let text = inspector_protocol_message_text(view);
        return serde_json::from_str(&text);
    }

    let text = inspector_protocol_message_text(view);
    serde_json::from_str(&text)
}

/// Materializes an Inspector string view for diagnostics and uncommon text
/// decoding paths. Callers on the ASCII JSON fast path should avoid this so a
/// successful protocol message does not allocate an intermediate `String`.
pub fn inspector_protocol_message_text(view: v8::inspector::StringView<'_>) -> String {
    if let Some(bytes) = view.characters8() {
        return bytes.iter().copied().map(char::from).collect();
    }
    String::from_utf16_lossy(
        view.characters16()
            .expect("a non-8-bit Inspector StringView must expose UTF-16 units"),
    )
}

pub fn v8_json_parse<'s>(scope: &mut PinScope<'s, '_>, json: &str) -> Option<Local<'s, Value>> {
    let global = scope.get_current_context().global(scope);
    let json_object = get_static_property(scope, global, "JSON")
        .and_then(|value| Local::<Object>::try_from(value).ok())?;
    let parse = get_static_property(scope, json_object, "parse")
        .and_then(|value| Local::<Function>::try_from(value).ok())?;
    let json = v8_string(scope, json)?;
    parse.call(scope, json_object.into(), &[json.into()])
}

pub fn array_contains_strict(
    scope: &mut PinScope<'_, '_>,
    array: Local<'_, Array>,
    expected: Local<'_, Value>,
) -> bool {
    for index in 0..array.length() {
        if array
            .get_index(scope, index)
            .is_some_and(|candidate| candidate.strict_equals(expected))
        {
            return true;
        }
    }
    false
}

pub fn array_push_value(
    scope: &mut PinScope<'_, '_>,
    array: Local<'_, Array>,
    value: Local<'_, Value>,
) {
    let _ = array.set_index(scope, array.length(), value);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{decode_inspector_protocol_message, inspector_protocol_message_text};

    #[test]
    fn inspector_protocol_ascii_bytes_decode_without_an_intermediate_string() {
        let raw = br#"{"id":7,"result":{"value":"\u7f51\u6613"}}"#;
        let decoded = decode_inspector_protocol_message(v8::inspector::StringView::from(&raw[..]))
            .expect("valid Inspector JSON");

        assert_eq!(decoded, json!({"id": 7, "result": {"value": "网易"}}));
    }

    #[test]
    fn inspector_protocol_latin1_and_utf16_views_keep_existing_text_semantics() {
        let latin1 = b"{\"value\":\"caf\xe9\"}";
        let decoded =
            decode_inspector_protocol_message(v8::inspector::StringView::from(&latin1[..]))
                .expect("valid Latin-1 Inspector JSON");
        assert_eq!(decoded, json!({"value": "café"}));

        let utf16 = "{\"value\":\"中文\"}".encode_utf16().collect::<Vec<_>>();
        let decoded =
            decode_inspector_protocol_message(v8::inspector::StringView::from(&utf16[..]))
                .expect("valid UTF-16 Inspector JSON");
        assert_eq!(decoded, json!({"value": "中文"}));
    }

    #[test]
    fn inspector_protocol_diagnostics_preserve_latin1_and_utf16_message_text() {
        let latin1 = b"{broken caf\xe9";
        assert_eq!(
            inspector_protocol_message_text(v8::inspector::StringView::from(&latin1[..])),
            "{broken café"
        );

        let utf16 = "{broken 中文".encode_utf16().collect::<Vec<_>>();
        assert_eq!(
            inspector_protocol_message_text(v8::inspector::StringView::from(&utf16[..])),
            "{broken 中文"
        );
    }
}
