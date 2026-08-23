use std::time::Duration;

use super::*;

#[test]
fn dom_binding_stats_are_empty_when_flag_disabled() {
    record_dom_binding_operation("appendChild", Duration::from_micros(7));
    assert!(take_dom_binding_operation_stats().is_empty());
}

#[test]
fn promise_hook_stats_are_empty_when_flag_disabled() {
    record_promise_hook_init();
    record_promise_hook_resolve();
    record_promise_reaction_before();
    record_promise_reaction_after();
    assert_eq!(take_promise_hook_stats(), PromiseHookStats::default());
}
