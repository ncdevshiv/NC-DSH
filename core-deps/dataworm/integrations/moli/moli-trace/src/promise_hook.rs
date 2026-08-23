use std::cell::RefCell;

use crate::promise_hook_trace_enabled;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PromiseHookStats {
    pub init_count: u64,
    pub resolve_count: u64,
    pub reaction_before_count: u64,
    pub reaction_after_count: u64,
}

thread_local! {
    static PROMISE_HOOK_STATS: RefCell<PromiseHookStats> =
        const { RefCell::new(PromiseHookStats {
            init_count: 0,
            resolve_count: 0,
            reaction_before_count: 0,
            reaction_after_count: 0,
        }) };
}

pub fn record_promise_hook_init() {
    record_promise_hook(|stats| stats.init_count += 1);
}

pub fn record_promise_hook_resolve() {
    record_promise_hook(|stats| stats.resolve_count += 1);
}

pub fn record_promise_reaction_before() {
    record_promise_hook(|stats| stats.reaction_before_count += 1);
}

pub fn record_promise_reaction_after() {
    record_promise_hook(|stats| stats.reaction_after_count += 1);
}

pub fn take_promise_hook_stats() -> PromiseHookStats {
    if !promise_hook_trace_enabled() {
        return PromiseHookStats::default();
    }
    PROMISE_HOOK_STATS.with_borrow_mut(std::mem::take)
}

fn record_promise_hook(record: impl FnOnce(&mut PromiseHookStats)) {
    if !promise_hook_trace_enabled() {
        return;
    }
    PROMISE_HOOK_STATS.with_borrow_mut(record);
}
