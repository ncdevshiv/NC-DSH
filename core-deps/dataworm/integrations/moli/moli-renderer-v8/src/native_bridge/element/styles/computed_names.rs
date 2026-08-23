use crate::{
    document_runtime::DomHandle,
    native_bridge::JsContextHost,
    style_engine::{
        computed_longhand_count, computed_longhand_first_vendor_index, computed_longhand_name_at,
    },
};

use super::declaration::{ComputedStyleRead, StyleComputationContext, computed_style_applies};

pub(super) fn computed_property_count(
    runtime: &JsContextHost,
    handle: DomHandle,
    context: StyleComputationContext,
) -> usize {
    if !computed_style_applies(runtime, handle) {
        return 0;
    }
    computed_longhand_count()
        + sorted_custom_property_names(&ComputedStyleRead::new_with_context(
            runtime, handle, context,
        ))
        .len()
}

pub(super) fn computed_property_name_at(
    runtime: &JsContextHost,
    handle: DomHandle,
    context: StyleComputationContext,
    index: usize,
) -> Option<String> {
    if !computed_style_applies(runtime, handle) {
        return None;
    }

    let first_vendor = computed_longhand_first_vendor_index();
    if index < first_vendor {
        return computed_longhand_name_at(index).map(str::to_owned);
    }

    // Custom properties sort between ordinary and vendor-prefixed longhands.
    // Standard indices therefore stay O(1); only this short tail needs the
    // element's resolved custom-property set.
    let custom_names = sorted_custom_property_names(&ComputedStyleRead::new_with_context(
        runtime, handle, context,
    ));
    let custom_index = index - first_vendor;
    if let Some(name) = custom_names.get(custom_index) {
        return Some(name.clone());
    }
    computed_longhand_name_at(index - custom_names.len()).map(str::to_owned)
}

pub(super) fn computed_property_names(
    runtime: &JsContextHost,
    handle: DomHandle,
    context: StyleComputationContext,
) -> Vec<String> {
    if !computed_style_applies(runtime, handle) {
        return Vec::new();
    }
    let read = ComputedStyleRead::new_with_context(runtime, handle, context);
    computed_property_names_for_read(&read)
}

pub(super) fn computed_property_names_for_read(read: &ComputedStyleRead<'_>) -> Vec<String> {
    let first_vendor = computed_longhand_first_vendor_index();
    let mut names = Vec::with_capacity(computed_longhand_count());
    names.extend(
        (0..first_vendor)
            .filter_map(computed_longhand_name_at)
            .map(str::to_owned),
    );
    names.extend(sorted_custom_property_names(read));
    names.extend(
        (first_vendor..computed_longhand_count())
            .filter_map(computed_longhand_name_at)
            .map(str::to_owned),
    );
    names
}

fn sorted_custom_property_names(read: &ComputedStyleRead<'_>) -> Vec<String> {
    let mut names = read.custom_property_names();
    names.sort_unstable();
    names.dedup();
    names
}
