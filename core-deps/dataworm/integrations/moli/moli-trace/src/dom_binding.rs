use std::{cell::RefCell, collections::BTreeMap, time::Duration};

use crate::dom_binding_timing_enabled;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DomBindingOperationStats {
    pub op: &'static str,
    pub count: u64,
    pub total_us: u128,
    pub max_us: u128,
}

thread_local! {
    static DOM_BINDING_OPERATION_STATS: RefCell<BTreeMap<&'static str, DomBindingOperationStats>> =
        const { RefCell::new(BTreeMap::new()) };
}

pub fn record_dom_binding_operation(op: &'static str, elapsed: Duration) {
    if !dom_binding_timing_enabled() {
        return;
    }
    let elapsed_us = elapsed.as_micros();
    DOM_BINDING_OPERATION_STATS.with_borrow_mut(|stats| {
        let entry = stats.entry(op).or_insert_with(|| DomBindingOperationStats {
            op,
            ..DomBindingOperationStats::default()
        });
        entry.count += 1;
        entry.total_us += elapsed_us;
        entry.max_us = entry.max_us.max(elapsed_us);
    });
}

pub fn take_dom_binding_operation_stats() -> Vec<DomBindingOperationStats> {
    if !dom_binding_timing_enabled() {
        return Vec::new();
    }
    DOM_BINDING_OPERATION_STATS.with_borrow_mut(|stats| {
        std::mem::take(stats)
            .into_values()
            .filter(|stat| stat.count > 0)
            .collect()
    })
}
