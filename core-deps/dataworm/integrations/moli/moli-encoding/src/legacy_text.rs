use crate::encoding_for_label;

pub fn decode_text_for_legacy_web(bytes: &[u8], charset_label: Option<&str>) -> String {
    let encoding = charset_label
        .and_then(encoding_for_label)
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(bytes).0.into_owned()
}
