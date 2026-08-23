use data_url::mime::Mime as WebMime;

pub fn parse_mime(input: &str) -> Option<mime::Mime> {
    parse_web_mime(input).and_then(|mime| mime.to_string().parse().ok())
}

pub fn mime_essence(input: &str) -> Option<String> {
    parse_web_mime(input).map(|mime| mime_essence_from_parsed(&mime))
}

pub fn request_header_content_type_essence(input: &str) -> Option<String> {
    if input.contains(',') {
        return None;
    }
    parse_web_mime(input).map(|mime| mime_essence_from_parsed(&mime))
}

pub fn mime_charset(input: &str) -> Option<String> {
    mime_parameter(input, "charset")
}

pub fn mime_parameter(input: &str, name: &str) -> Option<String> {
    let name = name.to_ascii_lowercase();
    parse_web_mime(input)?
        .get_parameter(&name)
        .map(str::to_owned)
}

pub fn normalize_web_api_mime_type(raw: &str) -> String {
    if raw.is_empty() || raw.bytes().any(|byte| !(0x20..=0x7e).contains(&byte)) {
        return String::new();
    }
    raw.to_ascii_lowercase()
}

fn parse_web_mime(input: &str) -> Option<WebMime> {
    input.parse().ok()
}

fn mime_essence_from_parsed(mime: &WebMime) -> String {
    format!("{}/{}", mime.type_, mime.subtype)
}
