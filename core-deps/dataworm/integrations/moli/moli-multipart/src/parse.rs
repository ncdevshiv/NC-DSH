use crate::types::{
    DEFAULT_MULTIPART_PARSED_FILE_CONTENT_TYPE, MultipartFormDataEntry, MultipartHeaders,
};
use moli_header_field::{HeaderFieldTokenMode, HeaderFieldTokenizer};

pub fn parse_multipart_form_data(
    bytes: &[u8],
    boundary: &str,
) -> Option<Vec<MultipartFormDataEntry>> {
    // Blink only rejects an empty boundary before constructing
    // `MultipartParser`; the parser then matches the extracted UTF-8 bytes
    // literally. In particular, it does not enforce RFC 2046's 70-byte or
    // `bchars` restrictions. Keep that observable behavior for
    // `Body.formData()` compatibility.
    if boundary.is_empty() {
        return None;
    }
    let delimiter = format!("--{boundary}").into_bytes();
    let marker = {
        let mut marker = Vec::with_capacity(2 + delimiter.len());
        marker.extend_from_slice(b"\r\n");
        marker.extend_from_slice(&delimiter);
        marker
    };
    let mut offset = find_initial_multipart_boundary(bytes, &delimiter)?;
    let mut entries = Vec::new();

    loop {
        if let Some(next) = consume_prefix(bytes, offset, b"--") {
            let next = consume_transport_padding(bytes, next);
            return consume_multipart_epilogue(bytes, next).map(|_| entries);
        }
        offset = consume_transport_padding(bytes, offset);
        offset = consume_prefix(bytes, offset, b"\r\n")?;
        let headers_end = find_bytes(&bytes[offset..], b"\r\n\r\n")?;
        let headers = parse_multipart_headers(&bytes[offset..offset + headers_end])?;
        offset += headers_end + b"\r\n\r\n".len();

        let body_end = find_multipart_boundary_marker(&bytes[offset..], &marker)?;
        let body = bytes[offset..offset + body_end].to_vec();
        offset += body_end + b"\r\n".len();
        offset = consume_prefix(bytes, offset, &delimiter)?;
        let filename = headers.filename;
        let content_type = headers.content_type.unwrap_or_else(|| {
            if filename.is_some() {
                DEFAULT_MULTIPART_PARSED_FILE_CONTENT_TYPE.to_owned()
            } else {
                String::new()
            }
        });
        entries.push(MultipartFormDataEntry {
            name: headers.name,
            filename,
            content_type,
            body,
        });
    }
}

fn parse_multipart_headers(bytes: &[u8]) -> Option<MultipartHeaders> {
    let text = String::from_utf8_lossy(bytes);
    let mut disposition = None;
    let mut content_type = None;
    for line in text.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-disposition") {
            disposition = Some(value.trim().to_owned());
        } else if name.trim().eq_ignore_ascii_case("content-type") {
            content_type = Some(value.trim().to_ascii_lowercase());
        }
    }
    let (name, filename) = parse_content_disposition(disposition.as_deref()?)?;
    Some(MultipartHeaders {
        name,
        filename,
        content_type,
    })
}

fn parse_content_disposition(value: &str) -> Option<(String, Option<String>)> {
    let mut tokenizer = HeaderFieldTokenizer::new(value);
    let disposition = tokenizer.consume_token(HeaderFieldTokenMode::Normal)?;

    // RFC 2183 declares disposition types case-insensitive. Blink's Fetch
    // FormData loader instead compares ParsedContentDisposition::Type() with
    // the literal "form-data", so preserve that case-sensitive quirk.
    if disposition != "form-data" {
        return None;
    }

    let mut name = None;
    let mut filename = None;
    while !tokenizer.is_consumed() {
        if !tokenizer.consume(';') {
            return None;
        }
        let key = tokenizer.consume_token(HeaderFieldTokenMode::Normal)?;
        if !tokenizer.consume('=') {
            return None;
        }
        let parameter_value = tokenizer
            .consume_token_or_quoted_string(HeaderFieldTokenMode::Normal)?
            .into_owned();
        if key.eq_ignore_ascii_case("name") {
            // Blink searches parsed parameters in reverse, so duplicates are
            // last-wins. Values stay literal here: unlike the serializer,
            // Chromium's parser does not percent-decode name or filename.
            name = Some(parameter_value);
        } else if key.eq_ignore_ascii_case("filename") {
            filename = Some(parameter_value);
        }
    }

    Some((name?, filename))
}

fn consume_prefix(bytes: &[u8], offset: usize, prefix: &[u8]) -> Option<usize> {
    bytes[offset..]
        .starts_with(prefix)
        .then_some(offset + prefix.len())
}

fn consume_multipart_epilogue(bytes: &[u8], offset: usize) -> Option<usize> {
    chromium_close_delimiter_tail_is_valid(&bytes[offset..]).then_some(bytes.len())
}

fn consume_transport_padding(bytes: &[u8], mut offset: usize) -> usize {
    while bytes
        .get(offset)
        .is_some_and(|byte| matches!(*byte, b' ' | b'\t'))
    {
        offset += 1;
    }
    offset
}

pub(crate) fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_initial_multipart_boundary(bytes: &[u8], delimiter: &[u8]) -> Option<usize> {
    if bytes.starts_with(delimiter) {
        return Some(delimiter.len());
    }

    let mut marker = Vec::with_capacity(2 + delimiter.len());
    marker.extend_from_slice(b"\r\n");
    marker.extend_from_slice(delimiter);
    find_bytes(bytes, &marker).map(|offset| offset + marker.len())
}

fn find_multipart_boundary_marker(haystack: &[u8], marker: &[u8]) -> Option<usize> {
    let next = find_bytes(haystack, marker)?;
    let after = next + marker.len();
    multipart_boundary_suffix_is_valid(&haystack[after..]).then_some(next)
}

fn multipart_boundary_suffix_is_valid(suffix: &[u8]) -> bool {
    if let Some(after_close) = consume_prefix(suffix, 0, b"--") {
        let after_padding = consume_transport_padding(suffix, after_close);
        return chromium_close_delimiter_tail_is_valid(&suffix[after_padding..]);
    }
    let after_padding = consume_transport_padding(suffix, 0);
    suffix[after_padding..].starts_with(b"\r\n")
}

fn chromium_close_delimiter_tail_is_valid(tail: &[u8]) -> bool {
    // RFC 2046 does not permit a lone CR here. Blink's MultipartParser::Finish
    // deliberately accepts a missing or partial CRLF after a complete closing
    // "--", so a trailing CR must also succeed.
    tail.is_empty() || tail == b"\r" || tail.starts_with(b"\r\n")
}
