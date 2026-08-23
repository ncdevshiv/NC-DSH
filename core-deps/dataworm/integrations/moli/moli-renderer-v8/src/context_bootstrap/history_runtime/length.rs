use super::super::navigation_entry::{history_length_number, set_history_length};
use super::super::navigation_window::{
    runtime_top_window_owner, runtime_window_uses_top_level_history_model,
    window_history_for_holder,
};

pub(crate) fn increment_top_level_history_length_for_runtime_owner<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    owner: v8::Local<'s, v8::Object>,
) {
    if !runtime_window_uses_top_level_history_model(scope, owner)
        && let Some(child_history) = window_history_for_holder(scope, owner)
        && let Some(child_length) = history_length_number(scope, child_history)
    {
        set_top_level_history_length_at_least_for_runtime_owner(
            scope,
            owner,
            child_length.max(0.0),
        );
        return;
    }
    let top_window = runtime_top_window_owner(scope, owner);
    let Some(history) = window_history_for_holder(scope, top_window) else {
        return;
    };
    let current_length = history_length_number(scope, history)
        .unwrap_or(0.0)
        .max(0.0);
    set_history_length(scope, history, current_length + 1.0);
}

pub(crate) fn set_top_level_history_length_at_least_for_runtime_owner<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    owner: v8::Local<'s, v8::Object>,
    length: f64,
) {
    let top_window = runtime_top_window_owner(scope, owner);
    let Some(history) = window_history_for_holder(scope, top_window) else {
        return;
    };
    let current_length = history_length_number(scope, history)
        .unwrap_or(0.0)
        .max(0.0);
    set_history_length(scope, history, current_length.max(length.max(0.0)));
}
