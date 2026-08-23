use crate::sniffing::{MimeSniffingContext, computed_mime_type};

pub fn data_url_body_and_computed_mime_type(
    input: &str,
    context: MimeSniffingContext,
) -> Option<(Vec<u8>, String)> {
    let (body, supplied_mime_type) = data_url_body_and_mime_type(input)?;
    let computed_mime_type = computed_mime_type(Some(&supplied_mime_type), false, context, &body);
    Some((body, computed_mime_type))
}

pub fn data_url_body_and_mime_type(input: &str) -> Option<(Vec<u8>, String)> {
    let data_url = data_url::DataUrl::process(input).ok()?;
    let supplied_mime_type = data_url.mime_type().to_string();
    let (body, _) = data_url.decode_to_vec().ok()?;
    Some((body, supplied_mime_type))
}

pub fn data_url_mime_type(input: &str) -> Option<String> {
    let data_url = data_url::DataUrl::process(input).ok()?;
    Some(data_url.mime_type().to_string())
}
