use crate::util::v8_string;

use super::attributes::set_detached_element_attribute_value;
use super::iframe_content_cache::clear_detached_iframe_cached_context;

pub(in crate::native_bridge::document) fn navigate_detached_iframe_to_url<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    iframe: v8::Local<'s, v8::Object>,
    url: &str,
) {
    if let Some(url) = v8_string(scope, url)
        && set_detached_element_attribute_value(scope, iframe, "src", url.into())
    {
        clear_detached_iframe_cached_context(scope, iframe);
    }
}
