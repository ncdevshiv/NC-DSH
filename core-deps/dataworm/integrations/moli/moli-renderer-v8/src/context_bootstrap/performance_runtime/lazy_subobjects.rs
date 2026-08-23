use super::install::build_lazy_performance_subobject_in_current_realm;
use super::{PERFORMANCE_EVENT_COUNTS_SLOT, PERFORMANCE_NAVIGATION_SLOT, PERFORMANCE_TIMING_SLOT};
use crate::util::{get_private_value, set_private_value, v8str};
use anyhow::{Result, anyhow};

const MATERIALIZING_SLOT: &str = "__moliPerformanceSubobjectMaterializing";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PerformanceSubobject {
    Timing,
    Navigation,
    EventCounts,
}

impl PerformanceSubobject {
    pub(super) fn from_slot(slot: &str) -> Option<Self> {
        match slot {
            PERFORMANCE_TIMING_SLOT => Some(Self::Timing),
            PERFORMANCE_NAVIGATION_SLOT => Some(Self::Navigation),
            PERFORMANCE_EVENT_COUNTS_SLOT => Some(Self::EventCounts),
            _ => None,
        }
    }

    pub(super) const fn slot(self) -> &'static str {
        match self {
            Self::Timing => PERFORMANCE_TIMING_SLOT,
            Self::Navigation => PERFORMANCE_NAVIGATION_SLOT,
            Self::EventCounts => PERFORMANCE_EVENT_COUNTS_SLOT,
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Timing => "PerformanceTiming",
            Self::Navigation => "PerformanceNavigation",
            Self::EventCounts => "EventCounts",
        }
    }
}

pub(super) fn ensure_performance_subobject<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    performance: v8::Local<'s, v8::Object>,
    subobject: PerformanceSubobject,
) -> Result<v8::Local<'s, v8::Value>> {
    if let Some(value) = get_private_value(scope, performance, subobject.slot()) {
        return Ok(value);
    }
    let relevant_context = performance
        .get_creation_context(scope)
        .ok_or_else(|| anyhow!("Performance receiver has no creation context"))?;
    if relevant_context == scope.get_current_context() {
        return build_and_cache_in_current_realm(scope, performance, subobject);
    }

    let performance = v8::Global::new(scope, performance);
    let value = {
        let target_scope = &mut v8::ContextScope::new(scope, relevant_context);
        let target_performance = v8::Local::new(target_scope, &performance);
        let value = build_and_cache_in_current_realm(target_scope, target_performance, subobject)?;
        v8::Global::new(target_scope, value)
    };
    Ok(v8::Local::new(scope, &value))
}

fn build_and_cache_in_current_realm<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    performance: v8::Local<'s, v8::Object>,
    subobject: PerformanceSubobject,
) -> Result<v8::Local<'s, v8::Value>> {
    if let Some(active) = get_private_value(scope, performance, MATERIALIZING_SLOT) {
        let active = active
            .to_string(scope)
            .map(|value| value.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "unknown".to_owned());
        return Err(anyhow!(
            "reentrant {} materialization while building {active}",
            subobject.label()
        ));
    }
    set_private_value(
        scope,
        performance,
        MATERIALIZING_SLOT,
        v8str(scope, subobject.label()).into(),
    );
    let result = build_lazy_performance_subobject_in_current_realm(scope, performance, subobject);
    set_private_value(
        scope,
        performance,
        MATERIALIZING_SLOT,
        v8::undefined(scope).into(),
    );
    let value = result?;
    set_private_value(scope, performance, subobject.slot(), value);
    Ok(value)
}
