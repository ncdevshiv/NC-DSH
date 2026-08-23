use crate::{
    css_style::parse_css_declaration_list,
    document_runtime::DomHandle,
    util::{context_host_ptr_from_global_bridge, get_private_value},
};

use super::super::super::detached_owner_document_object;
use super::attributes::detached_element_attribute_value;
use super::iframe_content_cache::CHILD_DOCUMENT_CONTEXT_HANDLE_SLOT;

pub(super) fn detached_iframe_viewport_width<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    iframe: v8::Local<'s, v8::Object>,
) -> Option<f64> {
    if let Some(width) = detached_numeric_attribute(scope, iframe, "width") {
        return Some(width);
    }
    let owner_width = detached_iframe_owner_viewport_width(scope, iframe);
    if let Some(width) = detached_style_width_px(scope, iframe) {
        return Some(width);
    }
    if let Some(percent) = detached_style_width_percent(scope, iframe)
        && let Some(owner_width) = owner_width
    {
        return Some(owner_width * percent / 100.0);
    }
    owner_width
}

fn detached_iframe_owner_viewport_width<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    iframe: v8::Local<'s, v8::Object>,
) -> Option<f64> {
    let owner_document = detached_owner_document_object(scope, iframe)?;
    let child_handle_value =
        get_private_value(scope, owner_document, CHILD_DOCUMENT_CONTEXT_HANDLE_SLOT)?;
    let child_handle_bigint = v8::Local::<v8::BigInt>::try_from(child_handle_value).ok()?;
    let (child_handle_index, child_handle_lossless) = child_handle_bigint.u64_value();
    let child_handle =
        child_handle_lossless.then(|| DomHandle::new(child_handle_index as usize))?;
    let runtime_ptr = context_host_ptr_from_global_bridge(scope)?;
    unsafe { &*runtime_ptr }
        .dom_host()
        .get_attribute(child_handle, "width")
        .and_then(|value| parse_positive_css_number(&value))
}

fn detached_numeric_attribute<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
    name: &str,
) -> Option<f64> {
    detached_element_attribute_value(scope, element, name)
        .and_then(|value| parse_positive_css_number(&value))
}

fn detached_style_width_px<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
) -> Option<f64> {
    parse_css_px(&detached_style_width(scope, element)?)
}

fn detached_style_width_percent<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
) -> Option<f64> {
    parse_css_percent(&detached_style_width(scope, element)?)
}

fn detached_style_width<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
) -> Option<String> {
    let style = detached_element_attribute_value(scope, element, "style")?;
    parse_css_declaration_list(&style)
        .into_iter()
        .find_map(|entry| {
            entry
                .name
                .eq_ignore_ascii_case("width")
                .then_some(entry.value)
        })
}

fn parse_css_px(value: &str) -> Option<f64> {
    let value = value.trim();
    if value == "0" {
        return Some(0.0);
    }
    value
        .strip_suffix("px")?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn parse_css_percent(value: &str) -> Option<f64> {
    value
        .trim()
        .strip_suffix('%')?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn parse_positive_css_number(value: &str) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
}
