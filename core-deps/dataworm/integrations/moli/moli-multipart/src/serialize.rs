use crate::parse::find_bytes;
use crate::types::{MultipartFormDataPart, MultipartFormDataPartValue};

pub fn serialize_multipart_form_data_with_prefix(
    parts: &[MultipartFormDataPart],
    boundary_prefix: &str,
) -> (Vec<u8>, String) {
    let boundary = multipart_boundary(parts, boundary_prefix);
    let body = serialize_multipart_form_data_body(parts, &boundary);
    (body, format!("multipart/form-data; boundary={boundary}"))
}

pub(crate) fn serialize_multipart_form_data_body(
    parts: &[MultipartFormDataPart],
    boundary: &str,
) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
        body.extend_from_slice(escape_multipart_name(&part.name).as_bytes());
        body.push(b'"');
        match &part.value {
            MultipartFormDataPartValue::Text(value) => {
                body.extend_from_slice(b"\r\n\r\n");
                body.extend_from_slice(value.as_bytes());
            }
            MultipartFormDataPartValue::Blob {
                filename,
                content_type,
                body: value,
            } => {
                body.extend_from_slice(b"; filename=\"");
                body.extend_from_slice(escape_multipart_name(filename).as_bytes());
                body.push(b'"');
                let content_type = if content_type.is_empty() {
                    super::types::DEFAULT_MULTIPART_BLOB_CONTENT_TYPE
                } else {
                    content_type
                };
                body.extend_from_slice(b"\r\nContent-Type: ");
                body.extend_from_slice(content_type.as_bytes());
                body.extend_from_slice(b"\r\n\r\n");
                body.extend_from_slice(value);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    body
}

fn multipart_boundary(parts: &[MultipartFormDataPart], boundary_prefix: &str) -> String {
    multipart_boundary_with_candidate(parts, |salt| {
        multipart_boundary_candidate(parts, salt, boundary_prefix)
    })
}

pub(crate) fn multipart_boundary_with_candidate(
    parts: &[MultipartFormDataPart],
    mut candidate: impl FnMut(u64) -> String,
) -> String {
    let mut salt = 0u64;
    loop {
        let boundary = candidate(salt);
        if !multipart_boundary_collides(parts, &boundary) {
            return boundary;
        }
        salt = salt.wrapping_add(1);
    }
}

fn multipart_boundary_candidate(
    parts: &[MultipartFormDataPart],
    salt: u64,
    boundary_prefix: &str,
) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        hash_part(&mut hash, part.name.bytes());
        match &part.value {
            MultipartFormDataPartValue::Text(value) => {
                hash_part(&mut hash, value.bytes());
            }
            MultipartFormDataPartValue::Blob {
                filename,
                content_type,
                body,
            } => {
                hash_part(&mut hash, filename.bytes());
                hash_part(&mut hash, content_type.bytes());
                hash_part(&mut hash, body.iter().copied());
            }
        }
    }
    if salt > 0 {
        hash_part(&mut hash, salt.to_le_bytes());
    }
    format!("{boundary_prefix}{hash:016x}")
}

fn hash_part(hash: &mut u64, bytes: impl IntoIterator<Item = u8>) {
    for byte in bytes {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    *hash ^= 0;
    *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
}

pub(crate) fn multipart_boundary_collides(parts: &[MultipartFormDataPart], boundary: &str) -> bool {
    parts.iter().any(|part| {
        part.name.contains(boundary)
            || match &part.value {
                MultipartFormDataPartValue::Text(value) => value.contains(boundary),
                MultipartFormDataPartValue::Blob {
                    filename,
                    content_type,
                    body,
                } => {
                    filename.contains(boundary)
                        || content_type.contains(boundary)
                        || find_bytes(body, boundary.as_bytes()).is_some()
                }
            }
    })
}

pub(crate) fn escape_multipart_name(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\n' => escaped.push_str("%0A"),
            '\r' => escaped.push_str("%0D"),
            '"' => escaped.push_str("%22"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
