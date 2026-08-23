// Adapted from multra 1.1.0's MIT-licensed parser tests.
//
// Streaming, locking, and `Constraints` cases are intentionally not included:
// `moli-multipart` parses an already-buffered body and has no equivalent
// API for those implementation-specific behaviors.
//
// Browser-observable expectations are calibrated to Chromium 147, except where
// the current Fetch WPT requires a standards-compatible behavior that Chromium
// does not yet implement. Those differences from multra, Chromium, or the RFC
// grammar are documented in
// `docs/moli-multipart-multra-test-port-2026-07-28.md`.

use moli_multipart::{
    DEFAULT_MULTIPART_PARSED_FILE_CONTENT_TYPE, MultipartFormDataEntry, parse_multipart_form_data,
};

const BOUNDARY: &str = "X-BOUNDARY";

fn single_part(disposition: &str) -> Vec<u8> {
    format!(
        "--{BOUNDARY}\r\n\
         Content-Disposition: {disposition}\r\n\
         \r\n\
         body\r\n\
         --{BOUNDARY}--\r\n"
    )
    .into_bytes()
}

fn single_part_with_boundary(boundary: &str) -> Vec<u8> {
    format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"field\"\r\n\
         \r\n\
         body\r\n\
         --{boundary}--\r\n"
    )
    .into_bytes()
}

fn parse_single_part(disposition: &str) -> MultipartFormDataEntry {
    let entries = parse_multipart_form_data(&single_part(disposition), BOUNDARY).unwrap();
    assert_eq!(entries.len(), 1);
    entries.into_iter().next().unwrap()
}

#[test]
fn parses_multra_basic_fields_and_preserves_mixed_newlines() {
    let body = b"--X-BOUNDARY\r\n\
Content-Disposition: form-data; name=\"my_text_field\"\r\n\
\r\n\
abcd\r\n\
--X-BOUNDARY\r\n\
Content-Disposition: form-data; name=\"my_file_field\"; filename=\"a-text-file.txt\"\r\n\
Content-Type: text/plain\r\n\
\r\n\
Hello world\nHello\r\nWorld\rAgain\r\n\
--X-BOUNDARY--\r\n";

    let entries = parse_multipart_form_data(body, BOUNDARY).unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "my_text_field");
    assert_eq!(entries[0].filename, None);
    assert_eq!(entries[0].content_type, "");
    assert_eq!(entries[0].body, b"abcd");
    assert_eq!(entries[1].name, "my_file_field");
    assert_eq!(entries[1].filename.as_deref(), Some("a-text-file.txt"));
    assert_eq!(entries[1].content_type, "text/plain");
    assert_eq!(entries[1].body, b"Hello world\nHello\r\nWorld\rAgain");
}

#[test]
fn parses_multra_empty_body_to_match_fetch_wpt() {
    let entries = parse_multipart_form_data(b"--X-BOUNDARY--\r\n", BOUNDARY).unwrap();

    assert!(entries.is_empty());
}

#[test]
fn accepts_nonempty_boundaries_like_chromium() {
    let overlong_boundary = "a".repeat(71);
    for boundary in [
        "X-BOUNDARY",
        "------ABCDEFG",
        "internal space",
        "azAZ09'()+_,-./:=?",
        // These three violate RFC 2046's bchars, length, or trailing-space
        // restrictions, but Chromium accepts them when extracted from quoted
        // Content-Type boundary parameters.
        "abc@def",
        overlong_boundary.as_str(),
        "trailing space ",
    ] {
        let body = single_part_with_boundary(boundary);
        assert!(
            parse_multipart_form_data(&body, boundary).is_some(),
            "expected Chromium-compatible boundary {boundary:?} to parse"
        );
    }

    assert!(parse_multipart_form_data(b"----\r\n", "").is_none());
}

#[test]
fn accepts_transport_padding_and_rejects_non_padding_suffixes() {
    let body = b"--X-BOUNDARY \t \r\n\
Content-Disposition: form-data; name=\"my_text_field\"\r\n\
\r\n\
abcd\r\n\
--X-BOUNDARY     \r\n\
Content-Disposition: form-data; name=\"my_file_field\"; filename=\"a-text-file.txt\"\r\n\
Content-Type: text/plain\r\n\
\r\n\
Hello world\r\n\
--X-BOUNDARY--\t\t\t\r\n";

    let entries = parse_multipart_form_data(body, BOUNDARY).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].body, b"abcd");
    assert_eq!(entries[1].body, b"Hello world");

    let malformed = b"--X-BOUNDARY \t \r\n\
Content-Disposition: form-data; name=\"my_text_field\"\r\n\
\r\n\
abcd\r\n\
--X-BOUNDARYzz     \r\n\
Content-Disposition: form-data; name=\"my_file_field\"\r\n\
\r\n\
second body\r\n\
--X-BOUNDARY--\r\n";
    assert!(parse_multipart_form_data(malformed, BOUNDARY).is_none());
}

#[test]
fn rejects_boundary_prefix_with_invalid_suffix_before_truncating() {
    let body = b"--X-BOUNDARY\r\n\
Content-Disposition: form-data; name=\"file\"\r\n\
\r\n\
abc\r\n\
--X-BOUNDARY-not-a-real-delimiter\r\n\
def\r\n\
--X-BOUNDARY--\r\n";

    assert!(parse_multipart_form_data(body, BOUNDARY).is_none());
}

#[test]
fn rejects_truncated_boundary_at_eof() {
    let body = b"--X-BOUNDARY\r\n\
Content-Disposition: form-data; name=\"file\"\r\n\
\r\n\
abc\r\n\
--X-BOUNDARY";

    assert!(parse_multipart_form_data(body, BOUNDARY).is_none());
}

#[test]
fn ignores_rfc_2046_preamble_and_epilogue() {
    for preamble in [
        "ignored header\r\n",
        "\r\nignored header\r\n",
        "\r\n",
        "\r\n\r\n",
    ] {
        let body = format!(
            "{preamble}--{BOUNDARY}\r\n\
             Content-Disposition: form-data; name=\"my_text_field\"\r\n\
             \r\n\
             abcd\r\n\
             --{BOUNDARY}--\r\n\
             ignored epilogue"
        );
        let entries = parse_multipart_form_data(body.as_bytes(), BOUNDARY).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "my_text_field");
        assert_eq!(entries[0].body, b"abcd");
    }
}

#[test]
fn accepts_chromium_closing_delimiter_eof_cases_but_not_unframed_epilogue() {
    let close_at_eof = b"--X-BOUNDARY\r\n\
Content-Disposition: form-data; name=\"field\"\r\n\
\r\n\
body\r\n\
--X-BOUNDARY--";
    assert!(parse_multipart_form_data(close_at_eof, BOUNDARY).is_some());

    // Blink's MultipartParser::Finish accepts a partial CRLF after the full
    // close delimiter, even though a lone CR is not valid RFC 2046 framing.
    let partial_crlf = b"--X-BOUNDARY\r\n\
Content-Disposition: form-data; name=\"field\"\r\n\
\r\n\
body\r\n\
--X-BOUNDARY--\r";
    assert!(parse_multipart_form_data(partial_crlf, BOUNDARY).is_some());

    let malformed = b"--X-BOUNDARY\r\n\
Content-Disposition: form-data; name=\"field\"\r\n\
\r\n\
body\r\n\
--X-BOUNDARY--not-an-epilogue";
    assert!(parse_multipart_form_data(malformed, BOUNDARY).is_none());
}

#[test]
fn parses_content_disposition_parameter_compatibility_cases() {
    let cases = [
        (r#"form-data; name="my_field""#, "my_field", None),
        ("form-data; name=my_field", "my_field", None),
        ("form-data; name  =  my_field  ", "my_field", None),
        (
            "form-data; filename=file-name.txt; name=file",
            "file",
            Some("file-name.txt"),
        ),
        (
            r#"form-data; name="my;f;ield"; filename="file;name.txt""#,
            "my;f;ield",
            Some("file;name.txt"),
        ),
        (
            r#"form-data; name="my\"field\"name"; filename="file\"name.txt""#,
            r#"my"field"name"#,
            Some(r#"file"name.txt"#),
        ),
        (
            r#"form-data; NAME="my_field"; FILENAME="file-name.txt""#,
            "my_field",
            Some("file-name.txt"),
        ),
        (
            "form-data; name=\"কখগ\"; filename=\"你好.txt\"",
            "কখগ",
            Some("你好.txt"),
        ),
        (r#"form-data; name=""; filename="""#, "", Some("")),
    ];

    for (disposition, expected_name, expected_filename) in cases {
        let entry = parse_single_part(disposition);
        assert_eq!(entry.name, expected_name, "disposition: {disposition}");
        assert_eq!(
            entry.filename.as_deref(),
            expected_filename,
            "disposition: {disposition}"
        );
        assert_eq!(
            entry.content_type,
            if expected_filename.is_some() {
                DEFAULT_MULTIPART_PARSED_FILE_CONTENT_TYPE
            } else {
                ""
            }
        );
    }
}

#[test]
fn uses_the_last_duplicate_content_disposition_parameter_like_chromium() {
    let entry = parse_single_part(
        "form-data; name=first; filename=first.txt; NAME=second; FILENAME=second.txt",
    );

    assert_eq!(entry.name, "second");
    assert_eq!(entry.filename.as_deref(), Some("second.txt"));
}

#[test]
fn rejects_content_disposition_values_rejected_by_chromium() {
    for disposition in [
        "form-data; name=",
        r#"form-data; name="field";"#,
        r#"form-data; malformed; name="field""#,
        "form-data; name=two words",
        r#"FORM-DATA; name="field""#,
    ] {
        assert!(
            parse_multipart_form_data(&single_part(disposition), BOUNDARY).is_none(),
            "expected disposition {disposition:?} to be rejected"
        );
    }
}

#[test]
fn requires_form_data_disposition_with_name_parameter() {
    assert!(
        parse_multipart_form_data(
            &single_part(r#"form-data; filename="file-name.txt""#),
            BOUNDARY
        )
        .is_none()
    );
    assert!(
        parse_multipart_form_data(&single_part(r#"attachment; name="field""#), BOUNDARY).is_none()
    );
}

#[test]
fn ignores_filename_star_per_rfc_7578() {
    let entry = parse_single_part(
        r#"form-data; name="upload"; filename="fallback.txt"; filename*=UTF-8''%E4%BD%A0%E5%A5%BD.txt"#,
    );
    assert_eq!(entry.filename.as_deref(), Some("fallback.txt"));

    let entry =
        parse_single_part(r#"form-data; name="upload"; filename*=UTF-8''%E4%BD%A0%E5%A5%BD.txt"#);
    assert_eq!(entry.filename, None);
    assert_eq!(entry.body, b"body");
}

#[test]
fn preserves_percent_sequences_in_names_and_filenames_like_chromium() {
    let entry =
        parse_single_part(r#"form-data; name="my%20field"; filename="file%20name%22x.txt""#);
    assert_eq!(entry.name, "my%20field");
    assert_eq!(entry.filename.as_deref(), Some("file%20name%22x.txt"));

    let entry = parse_single_part(r#"form-data; name="discount%rate"; filename="100%.txt""#);
    assert_eq!(entry.name, "discount%rate");
    assert_eq!(entry.filename.as_deref(), Some("100%.txt"));
}
