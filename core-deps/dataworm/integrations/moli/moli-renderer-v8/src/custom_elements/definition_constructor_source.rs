use super::definition::CustomElementDefineError;
use anyhow::Result;

pub(super) fn constructor_source_is_non_constructable(
    scope: &mut v8::PinScope<'_, '_>,
    constructor_value: v8::Local<'_, v8::Value>,
) -> Result<bool, CustomElementDefineError> {
    if constructor_value.is_proxy() {
        return Ok(false);
    }
    let source = constructor_value
        .to_string(scope)
        .ok_or(CustomElementDefineError::PendingException)?
        .to_rust_string_lossy(scope);
    let source = source.trim_start();
    if source.starts_with("class") {
        return Ok(false);
    }
    if source.starts_with("async function") || source.starts_with("function*") {
        return Ok(true);
    }
    if source.starts_with("function") {
        return Ok(false);
    }
    Ok(source.contains("=>")
        || source.starts_with("async ")
        || source_looks_like_method_shorthand(source))
}

fn source_looks_like_method_shorthand(source: &str) -> bool {
    let mut chars = source.chars().peekable();
    let Some(first) = chars.peek().copied() else {
        return false;
    };
    if !is_js_identifier_start_heuristic(first) {
        return false;
    }
    while chars
        .peek()
        .is_some_and(|character| is_js_identifier_part_heuristic(*character))
    {
        chars.next();
    }
    while chars
        .peek()
        .is_some_and(|character| character.is_whitespace())
    {
        chars.next();
    }
    chars.peek().is_some_and(|character| *character == '(')
}

fn is_js_identifier_start_heuristic(character: char) -> bool {
    character == '_' || character == '$' || character.is_ascii_alphabetic()
}

fn is_js_identifier_part_heuristic(character: char) -> bool {
    is_js_identifier_start_heuristic(character) || character.is_ascii_digit()
}
