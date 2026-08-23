use crate::WebCryptoError;

// Product bound for ASN.1 key containers accepted from page-controlled input.
// Supported RSA/EC/OKP keys are far smaller than this, so values above the cap
// are treated as resource-abuse attempts before OpenSSL parses DER.
pub const MAX_DER_KEY_BYTES: usize = 64 * 1024;
pub const MAX_RAW_KEY_IMPORT_BYTES: usize = 1024 * 1024;
pub const MAX_KDF_PARAMETER_BYTES: usize = 1024 * 1024;
pub const MAX_DIGEST_OPERATION_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SIGNATURE_OPERATION_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RSA_OAEP_LABEL_BYTES: usize = 64 * 1024;

pub(crate) fn ensure_der_key_bytes(bytes: &[u8]) -> Result<(), WebCryptoError> {
    ensure_len(bytes, MAX_DER_KEY_BYTES)
}

pub(crate) fn ensure_raw_key_import_bytes(bytes: &[u8]) -> Result<(), WebCryptoError> {
    ensure_len(bytes, MAX_RAW_KEY_IMPORT_BYTES)
}

pub(crate) fn ensure_kdf_parameter_bytes(bytes: &[u8]) -> Result<(), WebCryptoError> {
    ensure_len(bytes, MAX_KDF_PARAMETER_BYTES)
}

pub(crate) fn ensure_digest_operation_bytes(bytes: &[u8]) -> Result<(), WebCryptoError> {
    ensure_len(bytes, MAX_DIGEST_OPERATION_BYTES)
}

pub(crate) fn ensure_signature_operation_bytes(bytes: &[u8]) -> Result<(), WebCryptoError> {
    ensure_len(bytes, MAX_SIGNATURE_OPERATION_BYTES)
}

pub(crate) fn ensure_rsa_oaep_label_bytes(bytes: &[u8]) -> Result<(), WebCryptoError> {
    ensure_len(bytes, MAX_RSA_OAEP_LABEL_BYTES)
}

fn ensure_len(bytes: &[u8], max_bytes: usize) -> Result<(), WebCryptoError> {
    (bytes.len() <= max_bytes)
        .then_some(())
        .ok_or(WebCryptoError::Operation)
}
