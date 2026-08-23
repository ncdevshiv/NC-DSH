use std::io::{self, Write};

use serde::Serialize;

use crate::BoundedJsonError;

pub fn to_string_with_limit<T>(value: &T, limit: usize) -> Result<String, BoundedJsonError>
where
    T: Serialize + ?Sized,
{
    let mut output = BoundedJsonBuffer::new(limit);
    if let Err(error) = serde_json::to_writer(&mut output, value) {
        return if output.limit_exceeded {
            Err(BoundedJsonError::LimitExceeded { limit })
        } else {
            Err(BoundedJsonError::Serialization(error))
        };
    }
    Ok(String::from_utf8(output.bytes).expect("serde_json must emit UTF-8"))
}

/// Escapes `value` as one JSON string between an already-encoded prefix and
/// suffix, reusing the value's allocation when its capacity permits.
///
/// This is intended for typed protocol boundaries that have already validated
/// the surrounding JSON shape. The prefix must end immediately before the
/// string contents and the suffix must begin immediately after them; callers
/// remain responsible for including the opening and closing quotes.
pub fn json_string_between_with_limit(
    value: String,
    prefix: &str,
    suffix: &str,
    limit: usize,
) -> Result<String, BoundedJsonError> {
    let escaped_len = value.as_bytes().iter().try_fold(0usize, |total, byte| {
        total.checked_add(json_escaped_byte_len(*byte))
    });
    let Some(escaped_len) = escaped_len else {
        return Err(BoundedJsonError::LimitExceeded { limit });
    };
    let Some(output_len) = prefix
        .len()
        .checked_add(escaped_len)
        .and_then(|len| len.checked_add(suffix.len()))
    else {
        return Err(BoundedJsonError::LimitExceeded { limit });
    };
    if output_len > limit {
        return Err(BoundedJsonError::LimitExceeded { limit });
    }

    let mut output = value.into_bytes();
    let input_len = output.len();
    output.reserve(output_len.saturating_sub(input_len));
    output.resize(output_len, 0);

    let mut read = input_len;
    let mut write = prefix.len() + escaped_len;
    while read > 0 {
        read -= 1;
        let byte = output[read];
        match byte {
            b'"' => write_escaped_bytes(&mut output, &mut write, b"\\\""),
            b'\\' => write_escaped_bytes(&mut output, &mut write, b"\\\\"),
            b'\x08' => write_escaped_bytes(&mut output, &mut write, b"\\b"),
            b'\t' => write_escaped_bytes(&mut output, &mut write, b"\\t"),
            b'\n' => write_escaped_bytes(&mut output, &mut write, b"\\n"),
            b'\x0c' => write_escaped_bytes(&mut output, &mut write, b"\\f"),
            b'\r' => write_escaped_bytes(&mut output, &mut write, b"\\r"),
            0x00..=0x1f => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let escaped = [
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    HEX[(byte >> 4) as usize],
                    HEX[(byte & 0xf) as usize],
                ];
                write_escaped_bytes(&mut output, &mut write, &escaped);
            }
            _ => {
                write -= 1;
                output[write] = byte;
            }
        }
    }
    debug_assert_eq!(write, prefix.len());
    output[..prefix.len()].copy_from_slice(prefix.as_bytes());
    output[output_len - suffix.len()..].copy_from_slice(suffix.as_bytes());

    Ok(String::from_utf8(output).expect("escaping a Rust String must preserve UTF-8"))
}

fn json_escaped_byte_len(byte: u8) -> usize {
    match byte {
        b'"' | b'\\' | b'\x08' | b'\t' | b'\n' | b'\x0c' | b'\r' => 2,
        0x00..=0x1f => 6,
        _ => 1,
    }
}

fn write_escaped_bytes(output: &mut [u8], write: &mut usize, escaped: &[u8]) {
    *write -= escaped.len();
    output[*write..*write + escaped.len()].copy_from_slice(escaped);
}

struct BoundedJsonBuffer {
    bytes: Vec<u8>,
    limit: usize,
    limit_exceeded: bool,
}

impl BoundedJsonBuffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            limit_exceeded: false,
        }
    }
}

impl Write for BoundedJsonBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|len| len > self.limit)
        {
            self.limit_exceeded = true;
            return Err(io::Error::other("JSON output exceeds byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
