use super::super::super::detached_owner_document_object;

pub(super) fn resolve_detached_url_attribute<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
    value: &str,
) -> String {
    let base = detached_owner_document_object(scope, element)
        .map(|document| {
            super::super::super::detached_document_state_string(
                scope,
                document,
                "baseURI",
                "about:blank",
            )
        })
        .unwrap_or_else(|| "about:blank".to_owned());
    let base = url::Url::parse(&base).ok();
    url::Url::options()
        .base_url(base.as_ref())
        .parse(value)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| value.to_owned())
}
