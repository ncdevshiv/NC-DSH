use super::entries::{create_performance_entry, find_latest_performance_entry_start};
use super::*;
use crate::util::serialize_v8_iter_array;
use crate::webidl;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "Performance.mark")]
struct PerformanceMarkArgs {
    #[webidl(required)]
    name: String,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "Performance.clearMarks")]
struct PerformanceClearMarksArgs {
    name: Option<String>,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "Performance.measure")]
struct PerformanceMeasureArgs {
    #[webidl(required)]
    name: String,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "Performance.clearMeasures")]
struct PerformanceClearMeasuresArgs {
    name: Option<String>,
}

pub(in crate::context_bootstrap) fn performance_mark_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<PerformanceMarkArgs>(scope, &args) else {
        return;
    };
    let start_time = args
        .get(1)
        .to_object(scope)
        .and_then(|options| options.get(scope, v8str(scope, "startTime").into()))
        .and_then(|value| value.number_value(scope))
        .filter(|value| value.is_finite())
        .unwrap_or_else(|| {
            unix_epoch_millis()
                - performance_slot_number(scope, args.this(), PERFORMANCE_TIME_ORIGIN_SLOT)
                    .unwrap_or(0.0)
        });
    let entry = create_performance_entry(scope, "mark", &parsed.name, start_time, 0.0, None);
    push_performance_entry(scope, args.this(), entry);
    rv.set(entry.into());
}

pub(in crate::context_bootstrap) fn performance_clear_marks_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<PerformanceClearMarksArgs>(scope, &args) else {
        return;
    };
    let Some(entries) = performance_slot_array(scope, args.this(), PERFORMANCE_ENTRIES_SLOT) else {
        rv.set_undefined();
        return;
    };
    let mut next = Vec::new();
    for index in 0..entries.length() {
        let Some(entry) = entries.get_index(scope, index) else {
            continue;
        };
        let Ok(entry) = v8::Local::<v8::Object>::try_from(entry) else {
            continue;
        };
        let is_mark = performance_entry_slot_string(scope, entry, PERFORMANCE_ENTRY_TYPE_SLOT)
            .as_deref()
            == Some("mark");
        let keep = if !is_mark {
            true
        } else {
            match (
                &parsed.name,
                performance_entry_slot_string(scope, entry, PERFORMANCE_ENTRY_NAME_SLOT),
            ) {
                (Some(expected), Some(actual)) => actual != *expected,
                (Some(_), None) => true,
                (None, _) => false,
            }
        };
        if keep {
            next.push(entry);
        }
    }
    let next = serialize_v8_iter_array(scope, next).unwrap_or_else(|| v8::Array::new(scope, 0));
    set_performance_slot_value(scope, args.this(), PERFORMANCE_ENTRIES_SLOT, next.into());
    rv.set_undefined();
}

pub(in crate::context_bootstrap) fn performance_measure_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<PerformanceMeasureArgs>(scope, &args) else {
        return;
    };
    let now = unix_epoch_millis()
        - performance_slot_number(scope, args.this(), PERFORMANCE_TIME_ORIGIN_SLOT).unwrap_or(0.0);
    let (start, end, duration_value, detail) = if args.length() > 1
        && (args.get(1).is_string() || args.get(1).is_number())
    {
        let start = resolve_measure_boundary(scope, args.this(), args.get(1)).unwrap_or(0.0);
        let end = if args.length() > 2 && !args.get(2).is_undefined() {
            resolve_measure_boundary(scope, args.this(), args.get(2))
        } else {
            None
        };
        (start, end, None, None)
    } else {
        let options = args.get(1).to_object(scope);
        let start = options
            .and_then(|options| options.get(scope, v8str(scope, "start").into()))
            .and_then(|value| resolve_measure_boundary(scope, args.this(), value))
            .unwrap_or(0.0);
        let end = options
            .and_then(|options| options.get(scope, v8str(scope, "end").into()))
            .and_then(|value| resolve_measure_boundary(scope, args.this(), value));
        let duration_value = options
            .and_then(|options| options.get(scope, v8str(scope, "duration").into()))
            .and_then(|value| value.number_value(scope))
            .filter(|value| value.is_finite());
        let detail = options.and_then(|options| options.get(scope, v8str(scope, "detail").into()));
        (start, end, duration_value, detail)
    };

    let (start_time, duration) = match (end, duration_value) {
        (Some(end_time), Some(duration)) => (end_time - duration, duration),
        (Some(end_time), None) => (start, end_time - start),
        (None, Some(duration)) => (start, duration),
        (None, None) => (start, now - start),
    };
    let entry = create_performance_entry(
        scope,
        "measure",
        &parsed.name,
        start_time.max(0.0),
        duration.max(0.0),
        detail,
    );
    push_performance_entry(scope, args.this(), entry);
    rv.set(entry.into());
}

pub(in crate::context_bootstrap) fn performance_clear_measures_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<PerformanceClearMeasuresArgs>(scope, &args) else {
        return;
    };
    let Some(entries) = performance_slot_array(scope, args.this(), PERFORMANCE_ENTRIES_SLOT) else {
        rv.set_undefined();
        return;
    };
    let mut next = Vec::new();
    for index in 0..entries.length() {
        let Some(entry) = entries.get_index(scope, index) else {
            continue;
        };
        let Ok(entry) = v8::Local::<v8::Object>::try_from(entry) else {
            continue;
        };
        let is_measure = performance_entry_slot_string(scope, entry, PERFORMANCE_ENTRY_TYPE_SLOT)
            .as_deref()
            == Some("measure");
        let keep = if !is_measure {
            true
        } else {
            match (
                &parsed.name,
                performance_entry_slot_string(scope, entry, PERFORMANCE_ENTRY_NAME_SLOT),
            ) {
                (Some(expected), Some(actual)) => actual != *expected,
                (Some(_), None) => true,
                (None, _) => false,
            }
        };
        if keep {
            next.push(entry);
        }
    }
    let next = serialize_v8_iter_array(scope, next).unwrap_or_else(|| v8::Array::new(scope, 0));
    set_performance_slot_value(scope, args.this(), PERFORMANCE_ENTRIES_SLOT, next.into());
    rv.set_undefined();
}

fn resolve_measure_boundary<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    performance: v8::Local<'s, v8::Object>,
    value: v8::Local<'s, v8::Value>,
) -> Option<f64> {
    if let Some(number) = value.number_value(scope).filter(|value| value.is_finite()) {
        return Some(number);
    }
    let name = value.to_string(scope)?.to_rust_string_lossy(scope);
    match name.as_str() {
        "navigationStart" | "fetchStart" => Some(0.0),
        _ => find_latest_performance_entry_start(scope, performance, &name),
    }
}
