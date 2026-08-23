use crate::util::v8str;

use super::super::*;

fn delete_own_properties<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    wrapper: v8::Local<'s, v8::Object>,
    names: &[&'static str],
) {
    for name in names {
        let _ = wrapper.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge) fn install_specialized_instance_properties<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    wrapper: v8::Local<'s, v8::Object>,
    groups: RuntimeInstallGroups,
) {
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_object(scope, wrapper) else {
        return;
    };
    let runtime = unsafe { &mut *runtime_ptr };
    if runtime.dom_host().is_html_element_named(handle, "body") {
        delete_own_properties(scope, wrapper, BODY_LEGACY_PROTOTYPE_ACCESSORS);
    }
    if groups.svg_rect_animated_lengths {
        delete_own_properties(scope, wrapper, &["x", "y", "width", "height", "rx", "ry"]);
        delete_own_properties(scope, wrapper, &["transform"]);
    }
    if groups.svg_geometry_path_length {
        delete_own_properties(scope, wrapper, &["pathLength"]);
    }
    if groups.svg_text_positioning_lists {
        delete_own_properties(scope, wrapper, &["x", "y", "dx", "dy", "rotate"]);
    }
    if groups.svg_pattern_transform {
        delete_own_properties(scope, wrapper, &["patternTransform"]);
    }
    if groups.svg_gradient_transform {
        delete_own_properties(scope, wrapper, &["gradientTransform"]);
    }
}
