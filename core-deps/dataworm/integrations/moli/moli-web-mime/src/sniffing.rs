use crate::classification::{is_dom_parser_xml_mime, is_html_document_mime};
use crate::parse::mime_essence;

pub const RESOURCE_HEADER_BYTE_LIMIT: usize = 1445;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MimeSniffingContext {
    Browsing,
    Image,
    AudioVideo,
    Plugin,
    Style,
    Script,
    Font,
    TextTrack,
    CacheManifest,
}

pub fn resource_header(data: &[u8]) -> &[u8] {
    let limit = data.len().min(RESOURCE_HEADER_BYTE_LIMIT);
    &data[..limit]
}

pub fn computed_mime_type(
    supplied_content_type: Option<&str>,
    no_sniff: bool,
    context: MimeSniffingContext,
    data: &[u8],
) -> String {
    let header = resource_header(data);
    let supplied = supplied_content_type.and_then(mime_essence);

    match context {
        MimeSniffingContext::Browsing => computed_browsing_mime_type(
            supplied_content_type,
            supplied.as_deref(),
            no_sniff,
            header,
        ),
        MimeSniffingContext::Image => computed_image_mime_type(supplied.as_deref(), header),
        MimeSniffingContext::AudioVideo => {
            computed_audio_video_mime_type(supplied.as_deref(), header)
        }
        MimeSniffingContext::Plugin => {
            supplied.unwrap_or_else(|| "application/octet-stream".into())
        }
        MimeSniffingContext::Style => match supplied {
            Some(supplied) => supplied,
            None if no_sniff => "application/octet-stream".into(),
            None => "text/css".into(),
        },
        MimeSniffingContext::Script => supplied.unwrap_or_else(|| "text/javascript".into()),
        MimeSniffingContext::Font => computed_font_mime_type(supplied.as_deref(), header),
        MimeSniffingContext::TextTrack => "text/vtt".into(),
        MimeSniffingContext::CacheManifest => "text/cache-manifest".into(),
    }
}

pub fn sniff_unknown_mime_type(data: &[u8], sniff_scriptable: bool) -> String {
    let header = resource_header(data);
    if sniff_scriptable && let Some(mime) = sniff_scriptable_mime_type(header) {
        return mime.into();
    }
    if let Some(mime) = sniff_image_mime_type(header) {
        return mime.into();
    }
    if let Some(mime) = sniff_audio_video_mime_type(header) {
        return mime.into();
    }
    if let Some(mime) = sniff_archive_mime_type(header) {
        return mime.into();
    }
    sniff_text_or_binary(header).into()
}

pub fn sniff_text_or_binary(data: &[u8]) -> &'static str {
    let header = resource_header(data);
    if has_prefix(header, &[0xFE, 0xFF]) || has_prefix(header, &[0xFF, 0xFE]) {
        return "text/plain";
    }
    if has_prefix(header, &[0xEF, 0xBB, 0xBF]) {
        return "text/plain";
    }
    if header.iter().any(|byte| is_binary_data_byte(*byte)) {
        "application/octet-stream"
    } else {
        "text/plain"
    }
}

pub fn sniff_image_mime_type(data: &[u8]) -> Option<&'static str> {
    let header = resource_header(data);
    if has_prefix(header, &[0x00, 0x00, 0x01, 0x00])
        || has_prefix(header, &[0x00, 0x00, 0x02, 0x00])
    {
        return Some("image/x-icon");
    }
    if has_prefix(header, b"BM") {
        return Some("image/bmp");
    }
    if has_prefix(header, b"GIF87a") || has_prefix(header, b"GIF89a") {
        return Some("image/gif");
    }
    if header.len() >= 14 && &header[..4] == b"RIFF" && &header[8..14] == b"WEBPVP" {
        return Some("image/webp");
    }
    if has_prefix(header, b"\x89PNG\r\n\x1A\n") {
        return Some("image/png");
    }
    if has_prefix(header, &[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    None
}

pub fn sniff_audio_video_mime_type(data: &[u8]) -> Option<&'static str> {
    let header = resource_header(data);
    if riff_type(header, b"AIFF") {
        return Some("audio/aiff");
    }
    if has_prefix(header, b"ID3") || matches_mp3_header(header) {
        return Some("audio/mpeg");
    }
    if has_prefix(header, b"OggS\0") {
        return Some("application/ogg");
    }
    if has_prefix(header, b"MThd\0\0\0\x06") {
        return Some("audio/midi");
    }
    if riff_type(header, b"AVI ") {
        return Some("video/avi");
    }
    if riff_type(header, b"WAVE") {
        return Some("audio/wave");
    }
    if matches_mp4(header) {
        return Some("video/mp4");
    }
    if matches_webm(header) {
        return Some("video/webm");
    }
    None
}

pub fn sniff_font_mime_type(data: &[u8]) -> Option<&'static str> {
    let header = resource_header(data);
    if header.len() >= 36 && &header[34..36] == b"LP" {
        return Some("application/vnd.ms-fontobject");
    }
    if has_prefix(header, &[0x00, 0x01, 0x00, 0x00]) {
        return Some("font/ttf");
    }
    if has_prefix(header, b"OTTO") {
        return Some("font/otf");
    }
    if has_prefix(header, b"ttcf") {
        return Some("font/collection");
    }
    if has_prefix(header, b"wOFF") {
        return Some("font/woff");
    }
    if has_prefix(header, b"wOF2") {
        return Some("font/woff2");
    }
    None
}

pub fn sniff_archive_mime_type(data: &[u8]) -> Option<&'static str> {
    let header = resource_header(data);
    if has_prefix(header, &[0x1F, 0x8B, 0x08]) {
        return Some("application/x-gzip");
    }
    if has_prefix(header, b"PK\x03\x04") {
        return Some("application/zip");
    }
    if has_prefix(header, b"Rar!\x1A\x07\0") {
        return Some("application/x-rar-compressed");
    }
    None
}

fn computed_browsing_mime_type(
    supplied_content_type: Option<&str>,
    supplied: Option<&str>,
    no_sniff: bool,
    header: &[u8],
) -> String {
    if supplied.is_some_and(is_xml_or_html_mime) {
        return supplied.expect("checked supplied MIME").into();
    }
    if supplied.is_none_or(is_unknown_mime) {
        return sniff_unknown_mime_type(header, !no_sniff);
    }
    let supplied = supplied.expect("unknown MIME path handles missing supplied type");
    if no_sniff {
        return supplied.into();
    }
    if supplied_content_type.is_some_and(is_apache_bug_plain_text_type) {
        return sniff_text_or_binary(header).into();
    }
    if is_image_mime(supplied)
        && let Some(sniffed) = sniff_image_mime_type(header)
    {
        return sniffed.into();
    }
    if is_audio_video_mime(supplied)
        && let Some(sniffed) = sniff_audio_video_mime_type(header)
    {
        return sniffed.into();
    }
    supplied.into()
}

fn computed_image_mime_type(supplied: Option<&str>, header: &[u8]) -> String {
    if supplied.is_some_and(is_xml_mime) {
        return supplied.expect("checked supplied MIME").into();
    }
    sniff_image_mime_type(header)
        .or(supplied)
        .unwrap_or("application/octet-stream")
        .into()
}

fn computed_audio_video_mime_type(supplied: Option<&str>, header: &[u8]) -> String {
    if supplied.is_some_and(is_xml_mime) {
        return supplied.expect("checked supplied MIME").into();
    }
    sniff_audio_video_mime_type(header)
        .or(supplied)
        .unwrap_or("application/octet-stream")
        .into()
}

fn computed_font_mime_type(supplied: Option<&str>, header: &[u8]) -> String {
    if supplied.is_some_and(is_xml_mime) {
        return supplied.expect("checked supplied MIME").into();
    }
    sniff_font_mime_type(header)
        .or(supplied)
        .unwrap_or("application/octet-stream")
        .into()
}

fn sniff_scriptable_mime_type(data: &[u8]) -> Option<&'static str> {
    let header = skip_whitespace(data);
    let html_patterns = [
        b"<!DOCTYPE HTML".as_slice(),
        b"<HTML".as_slice(),
        b"<HEAD".as_slice(),
        b"<SCRIPT".as_slice(),
        b"<IFRAME".as_slice(),
        b"<H1".as_slice(),
        b"<DIV".as_slice(),
        b"<FONT".as_slice(),
        b"<TABLE".as_slice(),
        b"<A".as_slice(),
        b"<STYLE".as_slice(),
        b"<TITLE".as_slice(),
        b"<B".as_slice(),
        b"<BODY".as_slice(),
        b"<BR".as_slice(),
        b"<P".as_slice(),
        b"<!--".as_slice(),
    ];
    if html_patterns
        .iter()
        .any(|pattern| starts_with_case_insensitive_tag_pattern(header, pattern))
    {
        return Some("text/html");
    }
    if starts_with_case_sensitive(header, b"<?xml") {
        return Some("text/xml");
    }
    if starts_with_case_sensitive(header, b"%PDF-") {
        return Some("application/pdf");
    }
    if starts_with_case_sensitive(header, b"%!PS-Adobe-") {
        return Some("application/postscript");
    }
    if has_prefix(header, &[0xFE, 0xFF])
        || has_prefix(header, &[0xFF, 0xFE])
        || has_prefix(header, &[0xEF, 0xBB, 0xBF])
    {
        return Some("text/plain");
    }
    None
}

fn has_prefix(input: &[u8], prefix: &[u8]) -> bool {
    input.len() >= prefix.len() && &input[..prefix.len()] == prefix
}

fn riff_type(input: &[u8], form_type: &[u8; 4]) -> bool {
    input.len() >= 12 && &input[..4] == b"RIFF" && &input[8..12] == form_type
}

fn matches_mp4(input: &[u8]) -> bool {
    if input.len() < 12 {
        return false;
    }
    let box_size = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if box_size < 12 || input.len() < box_size || !box_size.is_multiple_of(4) {
        return false;
    }
    if &input[4..8] != b"ftyp" {
        return false;
    }
    if &input[8..11] == b"mp4" {
        return true;
    }
    let mut bytes_read = 16;
    while bytes_read + 3 <= box_size {
        if &input[bytes_read..bytes_read + 3] == b"mp4" {
            return true;
        }
        bytes_read += 4;
    }
    false
}

fn matches_webm(input: &[u8]) -> bool {
    if !has_prefix(input, &[0x1A, 0x45, 0xDF, 0xA3]) {
        return false;
    }
    let mut index = 4;
    while index + 1 < input.len() && index < 38 {
        if input[index] == 0x42 && input[index + 1] == 0x82 {
            index += 2;
            let Some(vint_size) = vint_size(&input[index..]) else {
                return false;
            };
            index += vint_size;
            if index >= input.len().saturating_sub(4) {
                return false;
            }
            return matches_padded_sequence(input, index, b"webm");
        }
        index += 1;
    }
    false
}

fn vint_size(input: &[u8]) -> Option<usize> {
    let first = *input.first()?;
    let mut mask = 0x80_u8;
    for size in 1..=8 {
        if first & mask != 0 {
            return Some(size);
        }
        mask >>= 1;
    }
    None
}

fn matches_padded_sequence(input: &[u8], offset: usize, pattern: &[u8]) -> bool {
    let mut index = offset;
    while index < input.len() && input[index] == 0x00 {
        index += 1;
    }
    input.get(index..index + pattern.len()) == Some(pattern)
}

fn matches_mp3_header(input: &[u8]) -> bool {
    if input.len() < 4 {
        return false;
    }
    if input[0] != 0xFF || input[1] & 0xE0 != 0xE0 {
        return false;
    }
    let layer = (input[1] & 0x06) >> 1;
    if layer == 0 {
        return false;
    }
    let bitrate = (input[2] & 0xF0) >> 4;
    if bitrate == 0 || bitrate == 15 {
        return false;
    }
    let sample_rate = (input[2] & 0x0C) >> 2;
    sample_rate != 3
}

fn skip_whitespace(input: &[u8]) -> &[u8] {
    let mut index = 0;
    while input
        .get(index)
        .is_some_and(|byte| is_whitespace_byte(*byte))
    {
        index += 1;
    }
    &input[index..]
}

fn starts_with_case_sensitive(input: &[u8], pattern: &[u8]) -> bool {
    has_prefix(input, pattern)
}

fn starts_with_case_insensitive_tag_pattern(input: &[u8], pattern: &[u8]) -> bool {
    if input.len() < pattern.len() {
        return false;
    }
    if !input[..pattern.len()].eq_ignore_ascii_case(pattern) {
        return false;
    }
    if pattern == b"<!--" {
        return input
            .get(pattern.len())
            .is_some_and(|byte| is_tag_terminating_byte(*byte));
    }
    input
        .get(pattern.len())
        .is_some_and(|byte| is_tag_terminating_byte(*byte))
}

fn is_whitespace_byte(byte: u8) -> bool {
    matches!(byte, 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

fn is_tag_terminating_byte(byte: u8) -> bool {
    is_whitespace_byte(byte) || matches!(byte, b'/' | b'>')
}

fn is_binary_data_byte(byte: u8) -> bool {
    matches!(byte, 0x00..=0x08 | 0x0B | 0x0E..=0x1A | 0x1C..=0x1F)
}

fn is_xml_or_html_mime(mime: &str) -> bool {
    is_xml_mime(mime) || is_html_mime(mime)
}

fn is_xml_mime(mime: &str) -> bool {
    is_dom_parser_xml_mime(mime) || mime.ends_with("+xml")
}

fn is_html_mime(mime: &str) -> bool {
    is_html_document_mime(mime)
}

fn is_unknown_mime(mime: &str) -> bool {
    matches!(mime, "unknown/unknown" | "application/unknown" | "*/*")
}

fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/")
}

fn is_audio_video_mime(mime: &str) -> bool {
    mime.starts_with("audio/") || mime.starts_with("video/") || mime == "application/ogg"
}

fn is_apache_bug_plain_text_type(raw: &str) -> bool {
    matches!(
        raw,
        "text/plain"
            | "text/plain; charset=ISO-8859-1"
            | "text/plain; charset=iso-8859-1"
            | "text/plain; charset=UTF-8"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_header_limits_to_spec_recommended_prefix() {
        let data = vec![b'a'; RESOURCE_HEADER_BYTE_LIMIT + 10];

        assert_eq!(resource_header(&data).len(), RESOURCE_HEADER_BYTE_LIMIT);
    }

    #[test]
    fn unknown_type_sniffs_scriptable_types_when_allowed() {
        assert_eq!(
            computed_mime_type(
                Some("application/unknown"),
                false,
                MimeSniffingContext::Browsing,
                b" \n<HTML>"
            ),
            "text/html"
        );
        assert_eq!(
            computed_mime_type(
                Some("application/unknown"),
                true,
                MimeSniffingContext::Browsing,
                b" \n<HTML>"
            ),
            "text/plain"
        );
        assert_eq!(
            computed_mime_type(None, false, MimeSniffingContext::Browsing, b"%PDF-1.7"),
            "application/pdf"
        );
    }

    #[test]
    fn unknown_type_sniffs_image_audio_font_archive_and_text_binary() {
        assert_eq!(
            sniff_unknown_mime_type(b"\x89PNG\r\n\x1A\nrest", true),
            "image/png"
        );
        assert_eq!(
            sniff_unknown_mime_type(b"RIFF\0\0\0\0WAVErest", true),
            "audio/wave"
        );
        assert_eq!(sniff_font_mime_type(b"wOF2rest"), Some("font/woff2"));
        assert_eq!(
            sniff_unknown_mime_type(b"PK\x03\x04rest", true),
            "application/zip"
        );
        assert_eq!(sniff_unknown_mime_type(b"plain text", true), "text/plain");
        assert_eq!(
            sniff_unknown_mime_type(b"\0binary", true),
            "application/octet-stream"
        );
    }

    #[test]
    fn browsing_context_respects_supplied_type_no_sniff_and_apache_bug() {
        assert_eq!(
            computed_mime_type(
                Some("text/html; charset=utf-8"),
                false,
                MimeSniffingContext::Browsing,
                b"\0"
            ),
            "text/html"
        );
        assert_eq!(
            computed_mime_type(
                Some("image/png"),
                true,
                MimeSniffingContext::Browsing,
                b"GIF89arest"
            ),
            "image/png"
        );
        assert_eq!(
            computed_mime_type(
                Some("text/plain"),
                false,
                MimeSniffingContext::Browsing,
                b"\0binary"
            ),
            "application/octet-stream"
        );
    }

    #[test]
    fn context_specific_sniffing_matches_supported_subsets() {
        assert_eq!(
            computed_mime_type(
                Some("image/png"),
                false,
                MimeSniffingContext::Image,
                b"GIF89arest"
            ),
            "image/gif"
        );
        assert_eq!(
            computed_mime_type(None, true, MimeSniffingContext::Style, b"body{}"),
            "application/octet-stream"
        );
        assert_eq!(
            computed_mime_type(None, false, MimeSniffingContext::Style, b"body{}"),
            "text/css"
        );
        assert_eq!(
            computed_mime_type(None, false, MimeSniffingContext::Script, b""),
            "text/javascript"
        );
        assert_eq!(
            computed_mime_type(
                Some("not a mime type"),
                false,
                MimeSniffingContext::Script,
                b""
            ),
            "text/javascript"
        );
        assert_eq!(
            computed_mime_type(None, false, MimeSniffingContext::TextTrack, b""),
            "text/vtt"
        );
    }

    #[test]
    fn mp4_and_webm_signatures_are_detected() {
        assert_eq!(
            sniff_audio_video_mime_type(b"\0\0\0\x14ftypisom\0\0\0\0mp42"),
            Some("video/mp4")
        );
        assert_eq!(
            sniff_audio_video_mime_type(&[
                0x1A, 0x45, 0xDF, 0xA3, 0x42, 0x82, 0x84, b'w', b'e', b'b', b'm', 0x00,
            ]),
            Some("video/webm")
        );
    }
}
