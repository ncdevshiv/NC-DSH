use super::*;

#[test]
fn legacy_codes_match_web_idl_domexception_table() {
    assert_eq!(dom_exception_legacy_code("IndexSizeError"), 1);
    assert_eq!(dom_exception_legacy_code("NotFoundError"), 8);
    assert_eq!(dom_exception_legacy_code("InvalidStateError"), 11);
    assert_eq!(dom_exception_legacy_code("SecurityError"), 18);
    assert_eq!(dom_exception_legacy_code("AbortError"), 20);
    assert_eq!(dom_exception_legacy_code("DataCloneError"), 25);
}

#[test]
fn modern_names_do_not_receive_legacy_codes() {
    for name in [
        "DataError",
        "OperationError",
        "ConstraintError",
        "TransactionInactiveError",
        "VersionError",
        "UnknownError",
        "NotAllowedError",
        "EncodingError",
        "WebSocketError",
    ] {
        assert_eq!(dom_exception_legacy_code(name), 0, "{name}");
    }
}

#[test]
fn unknown_names_are_valid_domexception_names_with_zero_code() {
    assert!(!is_dom_exception_name("ProjectSpecificError"));
    assert_eq!(dom_exception_legacy_code("ProjectSpecificError"), 0);
    assert_eq!(dom_exception_default_message("ProjectSpecificError"), None);
}
