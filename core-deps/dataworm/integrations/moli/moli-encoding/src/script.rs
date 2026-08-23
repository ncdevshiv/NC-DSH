use encoding_rs::Encoding;

use crate::{charset_from_headers, encoding_for_label};

pub fn decode_classic_script_source(
    bytes: &[u8],
    headers: &[(String, String)],
    script_charset: Option<&str>,
    document_character_set: Option<&str>,
) -> String {
    let header_charset = charset_from_headers(headers);
    let encoding = Encoding::for_bom(bytes)
        .map(|(encoding, _)| encoding)
        .or_else(|| header_charset.as_deref().and_then(encoding_for_label))
        .or_else(|| script_charset.and_then(encoding_for_label))
        .or_else(|| document_character_set.and_then(encoding_for_label))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode_with_bom_removal(bytes).0.into_owned()
}
