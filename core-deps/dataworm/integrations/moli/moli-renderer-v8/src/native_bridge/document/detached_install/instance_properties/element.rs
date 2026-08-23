use crate::{native_bridge::element::BODY_LEGACY_PROTOTYPE_ACCESSORS, util::v8str};

use super::*;

pub(in crate::native_bridge::document) fn install_detached_element_instance_properties<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
) {
    install_detached_node_core_instance_properties(scope, object);
    install_detached_parent_node_instance_properties(scope, object);
    install_detached_non_document_type_child_node_instance_properties(scope, object);
    let Some(local_name) = detached_element_local_name(scope, object) else {
        return;
    };
    let moved_properties: &[&str] = match local_name.to_ascii_lowercase().as_str() {
        "li" => &["value"],
        "ol" => &["start", "reversed", "type"],
        "optgroup" => &["disabled"],
        "details" => &["open"],
        "meta" => &["content", "httpEquiv"],
        "title" => &["text"],
        "body" => BODY_LEGACY_PROTOTYPE_ACCESSORS,
        "button" => &[
            "disabled",
            "formAction",
            "formEnctype",
            "formMethod",
            "formNoValidate",
            "formTarget",
            "type",
            "commandForElement",
            "popoverTargetElement",
            "popoverTargetAction",
            "interestForElement",
            "required",
            "value",
        ],
        "fieldset" => &["disabled", "type", "elements"],
        "datalist" => &["options"],
        "legend" => &["form"],
        "output" => &["type"],
        "meter" => &["value", "min", "max", "low", "high", "optimum"],
        "progress" => &["value", "max", "position"],
        "input" => &[
            "accept",
            "alt",
            "defaultChecked",
            "defaultValue",
            "disabled",
            "dirName",
            "files",
            "formAction",
            "formEnctype",
            "formMethod",
            "formNoValidate",
            "formTarget",
            "height",
            "list",
            "maxLength",
            "max",
            "minLength",
            "min",
            "multiple",
            "pattern",
            "placeholder",
            "readOnly",
            "required",
            "size",
            "src",
            "step",
            "type",
            "valueAsDate",
            "valueAsNumber",
            "value",
            "width",
            "checked",
            "indeterminate",
        ],
        "textarea" => &[
            "disabled",
            "dirName",
            "maxLength",
            "minLength",
            "required",
            "textLength",
            "type",
            "cols",
            "rows",
            "wrap",
            "placeholder",
            "readOnly",
            "defaultValue",
            "value",
        ],
        "thead" | "tbody" | "tfoot" => &["rows"],
        "tr" => &["rowIndex", "sectionRowIndex", "cells"],
        "td" | "th" => &["colSpan", "rowSpan", "cellIndex"],
        _ => &[],
    };
    for name in moved_properties {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_label_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    for name in ["htmlFor", "control", "form"] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_form_associated_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    for name in ["name", "form"] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_anchor_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    for name in [
        "href", "protocol", "host", "hostname", "port", "pathname", "search", "hash",
    ] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_text_replacement_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    let _ = object.delete(scope, v8str(scope, "text").into());
}

pub(in crate::native_bridge::document) fn install_detached_option_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    install_detached_text_replacement_instance_properties(scope, object);
    for name in [
        "defaultSelected",
        "disabled",
        "form",
        "index",
        "name",
        "selected",
    ] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_select_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    for name in [
        "length",
        "options",
        "selectedOptions",
        "selectedIndex",
        "value",
        "disabled",
        "multiple",
        "required",
        "size",
        "add",
        "remove",
        "item",
        "namedItem",
    ] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_image_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    let _ = object.delete(scope, v8str(scope, "src").into());
    let _ = object.delete(scope, v8str(scope, "srcset").into());
}

pub(in crate::native_bridge::document) fn install_detached_iframe_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    for name in ["src", "srcdoc", "contentDocument", "contentWindow"] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}
