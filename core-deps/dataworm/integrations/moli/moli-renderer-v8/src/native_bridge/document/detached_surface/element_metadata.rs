use super::*;

pub(in crate::native_bridge) struct DetachedElementMetadata {
    pub namespace_uri: Option<String>,
    pub prefix: Option<String>,
    pub local_name: String,
    pub tag_name: String,
}

fn detached_native_element_metadata<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<DetachedElementMetadata> {
    let runtime_ptr = crate::util::context_host_ptr_from_global_bridge(scope)?;
    let handle = detached_native_handle(scope, node)?;
    let dom_host = unsafe { &*runtime_ptr }.dom_host();
    let node = dom_host.node(handle)?;
    let element = node.as_element()?;
    let owner_document_is_html = node
        .owner_document()
        .and_then(|document| dom_host.node(document))
        .and_then(|document| document.as_document())
        .is_some_and(|document| document.is_html_document());
    let namespace_uri = (!element.namespace().is_empty()).then(|| element.namespace().to_owned());
    let local_name = if owner_document_is_html
        && namespace_uri.as_deref() == Some("http://www.w3.org/1999/xhtml")
    {
        element.local_name().to_ascii_lowercase()
    } else {
        element.local_name().to_owned()
    };
    let tag_name = if owner_document_is_html {
        element.node_name()
    } else {
        match element.prefix() {
            Some(prefix) if !prefix.is_empty() => format!("{prefix}:{}", element.local_name()),
            _ => element.local_name().to_owned(),
        }
    };
    Some(DetachedElementMetadata {
        namespace_uri,
        prefix: element.prefix().map(str::to_owned),
        local_name,
        tag_name,
    })
}

pub(in crate::native_bridge) fn detached_element_metadata<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<DetachedElementMetadata> {
    detached_native_element_metadata(scope, node).or_else(|| {
        Some(DetachedElementMetadata {
            namespace_uri: detached_state_string(scope, node, "namespaceURI"),
            prefix: detached_state_string(scope, node, "prefix"),
            local_name: detached_state_string(scope, node, "localName")?,
            tag_name: detached_state_string(scope, node, "nodeName").unwrap_or_default(),
        })
    })
}

pub(in crate::native_bridge) fn detached_element_namespace_uri<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<String> {
    detached_element_metadata(scope, node).and_then(|metadata| metadata.namespace_uri)
}

pub(in crate::native_bridge) fn detached_element_prefix<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<String> {
    detached_element_metadata(scope, node).and_then(|metadata| metadata.prefix)
}

pub(in crate::native_bridge) fn detached_element_local_name<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<String> {
    detached_element_metadata(scope, node).map(|metadata| metadata.local_name)
}

pub(in crate::native_bridge) fn detached_element_tag_name<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<String> {
    detached_element_metadata(scope, node).map(|metadata| metadata.tag_name)
}

pub(in crate::native_bridge) fn bridge_detached_element_namespace_uri_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(node) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        rv.set_null();
        return;
    };
    match detached_element_namespace_uri(scope, node) {
        Some(namespace) => set_string_return_value(scope, &mut rv, &namespace),
        None => rv.set_null(),
    }
}

pub(in crate::native_bridge) fn bridge_detached_element_prefix_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(node) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        rv.set_null();
        return;
    };
    match detached_element_prefix(scope, node) {
        Some(prefix) => set_string_return_value(scope, &mut rv, &prefix),
        None => rv.set_null(),
    }
}

pub(in crate::native_bridge) fn bridge_detached_element_local_name_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(node) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        rv.set_empty_string();
        return;
    };
    let local_name = detached_element_local_name(scope, node).unwrap_or_default();
    set_string_return_value(scope, &mut rv, &local_name);
}

pub(in crate::native_bridge) fn bridge_detached_element_tag_name_callback<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(node) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        rv.set_empty_string();
        return;
    };
    let tag_name = detached_element_tag_name(scope, node).unwrap_or_default();
    set_string_return_value(scope, &mut rv, &tag_name);
}
