use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use openssl::{memcmp, pkey::PKey, rand::rand_bytes, sign::Signer};

use crate::jwk::{decode_jwk_base64url, jwk_key_ops_allow_usages, jwk_use_allows_algorithm};
use crate::limits::{ensure_raw_key_import_bytes, ensure_signature_operation_bytes};
use crate::{HmacJsonWebKeyExport, HmacJsonWebKeyImport, WebCryptoError, WebCryptoHashAlgorithm};

pub const MAX_HMAC_KEY_LENGTH_BITS: usize = 65_536;
pub fn generate_hmac_key(
    hash: WebCryptoHashAlgorithm,
    length_bits: Option<usize>,
) -> Result<Vec<u8>, WebCryptoError> {
    let length_bits = length_bits.unwrap_or_else(|| hash.default_hmac_key_len_bytes() * 8);
    if length_bits == 0 || length_bits > MAX_HMAC_KEY_LENGTH_BITS {
        return Err(WebCryptoError::Operation);
    }
    let length_bytes = length_bits.div_ceil(8);
    let mut key_bytes = vec![0_u8; length_bytes];
    rand_bytes(&mut key_bytes).map_err(|_| WebCryptoError::Operation)?;
    truncate_key_bits(&mut key_bytes, length_bits);
    Ok(key_bytes)
}
pub fn hmac_signature(hash: WebCryptoHashAlgorithm, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    hmac_bytes(hash, key, data).ok()
}

pub fn verify_hmac(
    hash: WebCryptoHashAlgorithm,
    key: &[u8],
    data: &[u8],
    signature: &[u8],
) -> bool {
    if ensure_signature_operation_bytes(data).is_err()
        || ensure_signature_operation_bytes(signature).is_err()
    {
        return false;
    }
    let Ok(expected) = hmac_bytes(hash, key, data) else {
        return false;
    };
    expected.len() == signature.len() && memcmp::eq(&expected, signature)
}

fn hmac_bytes(
    hash: WebCryptoHashAlgorithm,
    key: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, WebCryptoError> {
    ensure_signature_operation_bytes(data)?;
    let key = PKey::hmac(key).map_err(|_| WebCryptoError::Operation)?;
    let mut signer =
        Signer::new(hash.message_digest(), &key).map_err(|_| WebCryptoError::Operation)?;
    signer.update(data).map_err(|_| WebCryptoError::Operation)?;
    signer.sign_to_vec().map_err(|_| WebCryptoError::Operation)
}

pub fn hmac_hash_from_jwk_alg(alg: &str) -> Option<WebCryptoHashAlgorithm> {
    match alg {
        "HS1" => Some(WebCryptoHashAlgorithm::Sha1),
        "HS256" => Some(WebCryptoHashAlgorithm::Sha256),
        "HS384" => Some(WebCryptoHashAlgorithm::Sha384),
        "HS512" => Some(WebCryptoHashAlgorithm::Sha512),
        _ => None,
    }
}
pub fn validate_hmac_import_key_bytes(
    mut key_bytes: Vec<u8>,
    length_bits: Option<usize>,
) -> Result<(Vec<u8>, usize), WebCryptoError> {
    if key_bytes.is_empty() {
        return Err(WebCryptoError::Data);
    }
    ensure_raw_key_import_bytes(&key_bytes)?;
    let Some(length_bits) = length_bits else {
        let length_bits = key_bytes.len() * 8;
        return Ok((key_bytes, length_bits));
    };
    if length_bits.div_ceil(8) != key_bytes.len() {
        return Err(WebCryptoError::Data);
    }
    truncate_key_bits(&mut key_bytes, length_bits);
    Ok((key_bytes, length_bits))
}

fn truncate_key_bits(bytes: &mut [u8], length_bits: usize) {
    let trailing_bits = length_bits % 8;
    if trailing_bits == 0 {
        return;
    }
    let last_byte_index = length_bits / 8;
    bytes[last_byte_index] &= 0xff_u8 << (8 - trailing_bits);
}

pub fn import_hmac_jwk_key(
    jwk: &HmacJsonWebKeyImport,
    expected_hash: WebCryptoHashAlgorithm,
    extractable: bool,
    usages: &[String],
) -> Result<Vec<u8>, WebCryptoError> {
    jwk.validate_resource_limits()?;
    if jwk.kty.as_deref() != Some("oct") {
        return Err(WebCryptoError::Data);
    }
    if let Some(alg) = jwk.alg.as_deref() {
        match hmac_hash_from_jwk_alg(alg) {
            Some(jwk_hash) if jwk_hash == expected_hash => {}
            _ => return Err(WebCryptoError::Data),
        }
    }
    if extractable && jwk.ext == Some(false) {
        return Err(WebCryptoError::Data);
    }
    jwk_use_allows_algorithm(jwk.public_key_use.as_deref(), "sig")?;
    jwk_key_ops_allow_usages(
        jwk.key_ops.as_deref(),
        usages,
        jwk.public_key_use.as_deref(),
    )?;
    let Some(k) = jwk.k.as_deref() else {
        return Err(WebCryptoError::Data);
    };
    decode_jwk_base64url(k)
}

pub fn export_hmac_jwk(
    hash: WebCryptoHashAlgorithm,
    key_bytes: &[u8],
    key_ops: Vec<String>,
    ext: bool,
) -> HmacJsonWebKeyExport {
    HmacJsonWebKeyExport {
        kty: "oct",
        k: URL_SAFE_NO_PAD.encode(key_bytes),
        alg: hash.jwk_hmac_alg(),
        key_ops,
        ext,
    }
}
