use super::*;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "IDB cursor open")]
struct IdbOpenCursorArgs<'s> {
    #[webidl(converter = "raw")]
    query: Option<v8::Local<'s, v8::Value>>,
    #[webidl(index = 1, converter = "raw")]
    direction: Option<v8::Local<'s, v8::Value>>,
}

pub(in crate::context_bootstrap::indexed_db::stores) struct ParsedOpenCursorArgs {
    pub(in crate::context_bootstrap::indexed_db::stores) query: Option<IdbKeyRangeQuery>,
    pub(in crate::context_bootstrap::indexed_db::stores) direction: CursorDirection,
}

pub(in crate::context_bootstrap::indexed_db::stores) fn parse_open_cursor_args<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    operation_name: &'static str,
) -> Option<ParsedOpenCursorArgs> {
    let parsed = webidl::parse_args::<IdbOpenCursorArgs<'s>>(scope, &args)?;
    let query_value = parsed.query.unwrap_or_else(|| v8::undefined(scope).into());
    let direction_value = parsed
        .direction
        .unwrap_or_else(|| v8::undefined(scope).into());
    let query = if query_value.is_null() {
        Ok(None)
    } else {
        parse_key_or_range(scope, query_value)
    };
    let query = match query {
        Err(_) => {
            let error =
                dom_exception_value(scope, invalid_query_message(operation_name), "DataError");
            scope.throw_exception(error);
            return None;
        }
        Ok(query) => query,
    };
    let direction = match parse_cursor_direction(scope, direction_value, operation_name) {
        Ok(direction) => direction,
        Err(error) => {
            webidl::throw_error(scope, &error);
            return None;
        }
    };
    Some(ParsedOpenCursorArgs { query, direction })
}

fn invalid_query_message(operation_name: &str) -> &'static str {
    match operation_name {
        "IDBObjectStore.openKeyCursor" | "IDBIndex.openKeyCursor" => {
            "Failed to execute 'openKeyCursor': the query is not a valid key or key range."
        }
        _ => "Failed to execute 'openCursor': the query is not a valid key or key range.",
    }
}
