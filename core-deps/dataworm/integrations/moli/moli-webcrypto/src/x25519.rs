use base64::{
    Engine,
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
};
use openssl::{
    derive::Deriver,
    pkey::{Id, PKey},
};
use zeroize::Zeroizing;

use crate::bits::truncate_derived_bits;
use crate::jwk::jwk_key_ops_allow_usages;
use crate::limits::ensure_der_key_bytes;
use crate::{OkpJsonWebKeyExport, OkpJsonWebKeyImport, WebCryptoError};

pub struct X25519KeyPair {
    pub private_key: Zeroizing<[u8; 32]>,
    pub public_key: [u8; 32],
}
const X25519_SPKI_PREFIX: [u8; 12] = [48, 42, 48, 5, 6, 3, 43, 101, 110, 3, 33, 0];
const X25519_PKCS8_PREFIX: [u8; 16] = [48, 46, 2, 1, 0, 48, 5, 6, 3, 43, 101, 110, 4, 34, 4, 32];
pub fn generate_x25519_key_pair() -> Result<X25519KeyPair, WebCryptoError> {
    let key = PKey::generate_x25519().map_err(|_| WebCryptoError::Operation)?;
    let private_bytes: [u8; 32] = key
        .raw_private_key()
        .map_err(|_| WebCryptoError::Operation)?
        .try_into()
        .map_err(|_| WebCryptoError::Operation)?;
    let private_key = Zeroizing::new(private_bytes);
    let public_key = key
        .raw_public_key()
        .map_err(|_| WebCryptoError::Operation)?
        .try_into()
        .map_err(|_| WebCryptoError::Operation)?;
    Ok(X25519KeyPair {
        private_key,
        public_key,
    })
}

pub fn x25519_public_key_from_private(private_key: &[u8; 32]) -> Result<[u8; 32], WebCryptoError> {
    let key = PKey::private_key_from_raw_bytes(private_key, Id::X25519)
        .map_err(|_| WebCryptoError::Operation)?;
    key.raw_public_key()
        .map_err(|_| WebCryptoError::Operation)?
        .try_into()
        .map_err(|_| WebCryptoError::Operation)
}

pub fn derive_x25519_bits(
    private_key: &[u8; 32],
    public_key: [u8; 32],
    length_bits: usize,
) -> Result<Vec<u8>, WebCryptoError> {
    let private_key = PKey::private_key_from_raw_bytes(private_key, Id::X25519)
        .map_err(|_| WebCryptoError::Operation)?;
    let public_key = PKey::public_key_from_raw_bytes(&public_key, Id::X25519)
        .map_err(|_| WebCryptoError::Operation)?;
    let mut deriver = Deriver::new(&private_key).map_err(|_| WebCryptoError::Operation)?;
    deriver
        .set_peer(&public_key)
        .map_err(|_| WebCryptoError::Operation)?;
    let secret = deriver
        .derive_to_vec()
        .map_err(|_| WebCryptoError::Operation)?;
    if secret.iter().all(|byte| *byte == 0) {
        return Err(WebCryptoError::Operation);
    }
    truncate_derived_bits(&secret, length_bits)
}

pub fn import_x25519_raw_public_key(bytes: &[u8]) -> Result<[u8; 32], WebCryptoError> {
    bytes.try_into().map_err(|_| WebCryptoError::Data)
}

pub fn import_x25519_spki_public_key(bytes: &[u8]) -> Result<[u8; 32], WebCryptoError> {
    ensure_der_key_bytes(bytes)?;
    if bytes.len() != X25519_SPKI_PREFIX.len() + 32 || !bytes.starts_with(&X25519_SPKI_PREFIX) {
        return Err(WebCryptoError::Data);
    }
    import_x25519_raw_public_key(&bytes[X25519_SPKI_PREFIX.len()..])
}

pub fn import_x25519_pkcs8_private_key(
    bytes: &[u8],
) -> Result<Zeroizing<[u8; 32]>, WebCryptoError> {
    ensure_der_key_bytes(bytes)?;
    if bytes.len() != X25519_PKCS8_PREFIX.len() + 32 || !bytes.starts_with(&X25519_PKCS8_PREFIX) {
        return Err(WebCryptoError::Data);
    }
    let mut private_key = Zeroizing::new([0_u8; 32]);
    private_key.copy_from_slice(&bytes[X25519_PKCS8_PREFIX.len()..]);
    Ok(private_key)
}

pub fn import_x25519_jwk_key(
    jwk: &OkpJsonWebKeyImport,
    extractable: bool,
    usages: &[String],
) -> Result<X25519ImportedKey, WebCryptoError> {
    jwk.validate_resource_limits()?;
    if jwk.kty.as_deref() != Some("OKP") || jwk.crv.as_deref() != Some("X25519") {
        return Err(WebCryptoError::Data);
    }
    if extractable && jwk.ext == Some(false) {
        return Err(WebCryptoError::Data);
    }
    // Chromium's X25519 JWK path delegates `use`/`key_ops` to the generic
    // usage verifier. That means `use: "sig"` is harmless for public-key
    // imports with empty usages, but still rejects private derive usages.
    jwk_key_ops_allow_usages(
        jwk.key_ops.as_deref(),
        usages,
        jwk.public_key_use.as_deref(),
    )?;
    let is_private_key = jwk.d.is_some();
    if !x25519_jwk_import_usages_are_valid(is_private_key, usages) {
        return Err(WebCryptoError::Syntax);
    }

    let Some(x) = jwk.x.as_deref() else {
        return Err(WebCryptoError::Data);
    };
    let public_key = decode_x25519_jwk_member(x)?;
    let Some(d) = jwk.d.as_deref() else {
        return Ok(X25519ImportedKey::Public(public_key));
    };
    let private_key = decode_x25519_jwk_private_member(d)?;
    if x25519_public_key_from_private(&private_key)? != public_key {
        return Err(WebCryptoError::Data);
    }
    Ok(X25519ImportedKey::Private(private_key))
}

fn x25519_jwk_import_usages_are_valid(is_private_key: bool, usages: &[String]) -> bool {
    if is_private_key {
        !usages.is_empty()
            && usages
                .iter()
                .all(|usage| matches!(usage.as_str(), "deriveKey" | "deriveBits"))
    } else {
        usages.is_empty()
    }
}
fn decode_x25519_jwk_member(value: &str) -> Result<[u8; 32], WebCryptoError> {
    URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .or_else(|_| URL_SAFE.decode(value.as_bytes()))
        .map_err(|_| WebCryptoError::Data)?
        .try_into()
        .map_err(|_| WebCryptoError::Data)
}

fn decode_x25519_jwk_private_member(value: &str) -> Result<Zeroizing<[u8; 32]>, WebCryptoError> {
    let decoded = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(value.as_bytes())
            .or_else(|_| URL_SAFE.decode(value.as_bytes()))
            .map_err(|_| WebCryptoError::Data)?,
    );
    if decoded.len() != 32 {
        return Err(WebCryptoError::Data);
    }
    let mut private_key = Zeroizing::new([0_u8; 32]);
    private_key.copy_from_slice(&decoded);
    Ok(private_key)
}

#[derive(Debug, Eq, PartialEq)]
pub enum X25519ImportedKey {
    Public([u8; 32]),
    Private(Zeroizing<[u8; 32]>),
}

pub fn export_x25519_spki_public_key(public_key: &[u8; 32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(X25519_SPKI_PREFIX.len() + public_key.len());
    bytes.extend_from_slice(&X25519_SPKI_PREFIX);
    bytes.extend_from_slice(public_key);
    bytes
}

pub fn export_x25519_pkcs8_private_key(private_key: &[u8; 32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(X25519_PKCS8_PREFIX.len() + private_key.len());
    bytes.extend_from_slice(&X25519_PKCS8_PREFIX);
    bytes.extend_from_slice(private_key);
    bytes
}

pub fn export_x25519_jwk_public_key(
    public_key: &[u8; 32],
    key_ops: Vec<String>,
    ext: bool,
) -> OkpJsonWebKeyExport {
    OkpJsonWebKeyExport {
        kty: "OKP",
        crv: "X25519",
        x: URL_SAFE_NO_PAD.encode(public_key),
        d: None,
        alg: None,
        key_ops,
        ext,
    }
}

pub fn export_x25519_jwk_private_key(
    private_key: &[u8; 32],
    key_ops: Vec<String>,
    ext: bool,
) -> Result<OkpJsonWebKeyExport, WebCryptoError> {
    Ok(OkpJsonWebKeyExport {
        kty: "OKP",
        crv: "X25519",
        x: URL_SAFE_NO_PAD.encode(x25519_public_key_from_private(private_key)?),
        d: Some(URL_SAFE_NO_PAD.encode(private_key)),
        alg: None,
        key_ops,
        ext,
    })
}
