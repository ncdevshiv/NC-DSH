use super::JsContextHost;
use crate::document_runtime::DomHandle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native_bridge::element) enum TrustedHtmlSink {
    ElementInnerHtml,
    ShadowRootInnerHtml,
    ElementOuterHtml,
    ElementSetHtmlUnsafe,
    ShadowRootSetHtmlUnsafe,
    ElementInsertAdjacentHtml,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native_bridge::element) enum TrustedScriptElementSink {
    InnerText,
    TextContent,
    Text,
}

impl TrustedScriptElementSink {
    fn name(self) -> &'static str {
        match self {
            Self::InnerText => "HTMLScriptElement innerText",
            Self::TextContent => "HTMLScriptElement textContent",
            Self::Text => "HTMLScriptElement text",
        }
    }

    pub(super) fn api_name(self) -> &'static str {
        match self {
            Self::InnerText => "innerText",
            Self::TextContent => "textContent",
            Self::Text => "text",
        }
    }

    fn null_is_empty(self) -> bool {
        !matches!(self, Self::Text)
    }
}

impl TrustedHtmlSink {
    fn name(self) -> &'static str {
        match self {
            Self::ElementInnerHtml => "Element innerHTML",
            Self::ShadowRootInnerHtml => "ShadowRoot innerHTML",
            Self::ElementOuterHtml => "Element outerHTML",
            Self::ElementSetHtmlUnsafe => "Element setHTMLUnsafe",
            Self::ShadowRootSetHtmlUnsafe => "ShadowRoot setHTMLUnsafe",
            Self::ElementInsertAdjacentHtml => "Element insertAdjacentHTML",
        }
    }

    fn api_name(self) -> &'static str {
        match self {
            Self::ElementInnerHtml | Self::ShadowRootInnerHtml => "innerHTML",
            Self::ElementOuterHtml => "outerHTML",
            Self::ElementSetHtmlUnsafe | Self::ShadowRootSetHtmlUnsafe => "setHTMLUnsafe",
            Self::ElementInsertAdjacentHtml => "insertAdjacentHTML",
        }
    }

    fn uses_legacy_null_to_empty_string(self) -> bool {
        matches!(
            self,
            Self::ElementInnerHtml | Self::ShadowRootInnerHtml | Self::ElementOuterHtml
        )
    }
}

pub(in crate::native_bridge::element) fn trusted_html_sink_string<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    runtime_ptr: *mut JsContextHost,
    value: v8::Local<'s, v8::Value>,
    sink: TrustedHtmlSink,
) -> Option<String> {
    let value = if sink.uses_legacy_null_to_empty_string() && value.is_null() {
        v8::String::empty(scope).into()
    } else {
        value
    };
    let requirements = unsafe { &*runtime_ptr }.trusted_types_for_script_requirements(scope);
    crate::context_bootstrap::trusted_html_string_or_throw(
        scope,
        value,
        requirements,
        sink.name(),
        sink.api_name(),
    )
}

pub(in crate::native_bridge::element) fn trusted_script_element_sink_string<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    runtime_ptr: *mut JsContextHost,
    value: v8::Local<'s, v8::Value>,
    sink: TrustedScriptElementSink,
) -> Option<String> {
    let value = if sink.null_is_empty() && value.is_null_or_undefined() {
        v8::String::empty(scope).into()
    } else {
        value
    };
    let requirements = unsafe { &*runtime_ptr }.trusted_types_for_script_requirements(scope);
    crate::context_bootstrap::trusted_script_string_or_type_error(
        scope,
        value,
        requirements,
        sink.name(),
        sink.api_name(),
    )
}

pub(in crate::native_bridge::element) fn trusted_script_url_sink_string<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    runtime_ptr: *mut JsContextHost,
    value: v8::Local<'s, v8::Value>,
) -> Option<String> {
    let requirements = unsafe { &*runtime_ptr }.trusted_types_for_script_requirements(scope);
    crate::context_bootstrap::trusted_script_url_string_or_throw(
        scope,
        value,
        requirements,
        "HTMLScriptElement src",
        "src",
    )
}

pub(crate) fn trusted_script_source_for_execution(
    scope: &mut v8::PinScope<'_, '_>,
    runtime_ptr: *mut JsContextHost,
    handle: DomHandle,
    source: &str,
) -> Option<String> {
    let runtime = unsafe { &*runtime_ptr };
    if !runtime.requires_trusted_types_for_script(scope) {
        return Some(source.to_owned());
    }
    let (trusted_source, sink) = runtime
        .dom_host()
        .node(handle)
        .and_then(|node| node.as_element())
        .filter(|element| element.is_script_element())
        .map(|element| {
            let sink = if element.namespace() == "http://www.w3.org/2000/svg" {
                "SVGScriptElement text"
            } else {
                "HTMLScriptElement text"
            };
            (element.script_text_internal_slot().to_owned(), sink)
        })?;
    if source == trusted_source {
        return Some(source.to_owned());
    }
    crate::context_bootstrap::trusted_script_string_for_script_element_execution(
        scope, source, sink,
    )
}
