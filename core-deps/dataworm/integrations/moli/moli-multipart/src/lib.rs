mod parse;
mod serialize;
mod types;

pub use parse::parse_multipart_form_data;
pub use serialize::serialize_multipart_form_data_with_prefix;
pub use types::{
    DEFAULT_MULTIPART_BLOB_CONTENT_TYPE, DEFAULT_MULTIPART_PARSED_FILE_CONTENT_TYPE,
    MultipartFormDataEntry, MultipartFormDataPart, MultipartFormDataPartValue,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialize::{
        escape_multipart_name, multipart_boundary_collides, multipart_boundary_with_candidate,
        serialize_multipart_form_data_body,
    };

    fn text_part(name: &str, value: &str) -> MultipartFormDataPart {
        MultipartFormDataPart {
            name: name.to_owned(),
            value: MultipartFormDataPartValue::Text(value.to_owned()),
        }
    }

    fn blob_part(
        name: &str,
        filename: &str,
        content_type: &str,
        body: &[u8],
    ) -> MultipartFormDataPart {
        MultipartFormDataPart {
            name: name.to_owned(),
            value: MultipartFormDataPartValue::Blob {
                filename: filename.to_owned(),
                content_type: content_type.to_owned(),
                body: body.to_vec(),
            },
        }
    }

    #[test]
    fn escape_multipart_name_uses_percent_encoding_for_header_parameters() {
        assert_eq!(
            escape_multipart_name("line\nbreak\r\"quote\""),
            "line%0Abreak%0D%22quote%22"
        );
        assert_eq!(escape_multipart_name("cafe-\u{e9}"), "cafe-\u{e9}");
    }

    #[test]
    fn multipart_boundary_selection_skips_colliding_candidate() {
        let parts = [text_part("field", "before--forced-boundary--after")];

        let selected = multipart_boundary_with_candidate(&parts, |salt| match salt {
            0 => "forced-boundary".to_owned(),
            1 => "safe-boundary".to_owned(),
            _ => panic!("selection should stop after the first non-colliding candidate"),
        });

        assert_eq!(selected, "safe-boundary");
    }

    #[test]
    fn multipart_boundary_collision_scans_names_text_and_blob_metadata() {
        let parts = [
            text_part("name-collision", "plain"),
            text_part("field", "value-collision"),
            blob_part("blob", "filename-collision.txt", "text/plain", b"body"),
            blob_part("typed", "file.txt", "application/collision", b"body"),
        ];

        assert!(multipart_boundary_collides(&parts, "name-collision"));
        assert!(multipart_boundary_collides(&parts, "value-collision"));
        assert!(multipart_boundary_collides(&parts, "filename-collision"));
        assert!(multipart_boundary_collides(&parts, "application/collision"));
        assert!(!multipart_boundary_collides(&parts, "missing-boundary"));
    }

    #[test]
    fn multipart_boundary_collision_scans_blob_bytes() {
        let parts = [blob_part(
            "file",
            "payload.bin",
            "application/octet-stream",
            b"\x00prefix--binary-boundary--suffix\xff",
        )];

        assert!(multipart_boundary_collides(&parts, "binary-boundary"));
        assert!(!multipart_boundary_collides(&parts, "text-boundary"));
    }

    #[test]
    fn multipart_blob_parts_default_empty_mime_type_to_octet_stream() {
        let parts = [blob_part("upload", "empty-type.bin", "", b"bytes")];

        let body =
            String::from_utf8(serialize_multipart_form_data_body(&parts, "boundary")).unwrap();

        assert_eq!(
            body,
            "--boundary\r\n\
             Content-Disposition: form-data; name=\"upload\"; filename=\"empty-type.bin\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n\
             bytes\r\n\
             --boundary--\r\n"
        );
    }

    #[test]
    fn parses_multipart_text_and_file_entries() {
        let body = b"--boundary\r\n\
Content-Disposition: form-data; name=\"field\"\r\n\
\r\n\
value\r\n\
--boundary\r\n\
content-disposition: form-data; name=\"upload\"; filename=\"file.json\"\r\n\
content-type: Application/JSON\r\n\
\r\n\
{\"ok\":true}\r\n\
--boundary--\r\n";

        let entries = parse_multipart_form_data(body, "boundary").unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "field");
        assert_eq!(entries[0].filename, None);
        assert_eq!(entries[0].body, b"value");
        assert_eq!(entries[1].name, "upload");
        assert_eq!(entries[1].filename.as_deref(), Some("file.json"));
        assert_eq!(entries[1].content_type, "application/json");
        assert_eq!(entries[1].body, br#"{"ok":true}"#);
    }

    #[test]
    fn parsed_file_parts_without_content_type_default_to_text_plain() {
        let body = b"--boundary\r\n\
Content-Disposition: form-data; name=\"field\"\r\n\
\r\n\
value\r\n\
--boundary\r\n\
Content-Disposition: form-data; name=\"upload\"; filename=\"file.txt\"\r\n\
\r\n\
file-body\r\n\
--boundary--\r\n";

        let entries = parse_multipart_form_data(body, "boundary").unwrap();

        assert_eq!(entries[0].filename, None);
        assert_eq!(entries[0].content_type, "");
        assert_eq!(entries[1].filename.as_deref(), Some("file.txt"));
        assert_eq!(
            entries[1].content_type,
            DEFAULT_MULTIPART_PARSED_FILE_CONTENT_TYPE
        );
    }

    #[test]
    fn parses_capitalized_boundary_values_case_sensitively() {
        let body = b"--Boundary_with_capital_letters\r\n\
Content-Type: application/json\r\n\
Content-Disposition: form-data; name=\"does_this_work\"\r\n\
\r\n\
YES\r\n\
--Boundary_with_capital_letters--\r\n";

        let entries = parse_multipart_form_data(body, "Boundary_with_capital_letters").unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "does_this_work");
        assert_eq!(entries[0].body, b"YES");
    }

    #[test]
    fn parses_transport_padding_after_boundary_delimiters() {
        let body = b"--boundary \t\r\n\
Content-Disposition: form-data; name=\"first\"\r\n\
\r\n\
one\r\n\
--boundary\t \r\n\
Content-Disposition: form-data; name=\"second\"\r\n\
\r\n\
two\r\n\
--boundary-- \t\r\n";

        let entries = parse_multipart_form_data(body, "boundary").unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "first");
        assert_eq!(entries[0].body, b"one");
        assert_eq!(entries[1].name, "second");
        assert_eq!(entries[1].body, b"two");
    }

    #[test]
    fn rejects_malformed_boundary_suffixes() {
        let body = b"--Boundary_with_capital_letters\r\n\
Content-Type: application/json\r\n\
Content-Disposition: form-data; name=\"does_this_work\"\r\n\
\r\n\
YES\r\n\
--Boundary_with_capital_letters-Random junk";

        assert!(parse_multipart_form_data(body, "Boundary_with_capital_letters").is_none());
    }

    #[test]
    fn parses_empty_multipart_body() {
        for body in [b"--boundary--".as_slice(), b"--boundary--\r\n"] {
            let entries = parse_multipart_form_data(body, "boundary").unwrap();
            assert!(entries.is_empty());
        }
    }

    #[test]
    fn preserves_percent_escaped_names_and_filenames_like_chromium() {
        let body = b"--boundary\r\n\
Content-Disposition: form-data; name=\"line%0Dbreak\"; filename=\"a%22b.txt\"\r\n\
\r\n\
body\r\n\
--boundary--\r\n";

        let entries = parse_multipart_form_data(body, "boundary").unwrap();

        assert_eq!(entries[0].name, "line%0Dbreak");
        assert_eq!(entries[0].filename.as_deref(), Some("a%22b.txt"));
    }

    #[test]
    fn rejects_boundary_prefixes_at_the_start_of_part_body_lines() {
        let body = b"--boundary\r\n\
Content-Disposition: form-data; name=\"field\"\r\n\
\r\n\
before\r\n--boundary-like text\r\n\
after\r\n\
--boundary--\r\n";

        assert!(parse_multipart_form_data(body, "boundary").is_none());
    }
}
