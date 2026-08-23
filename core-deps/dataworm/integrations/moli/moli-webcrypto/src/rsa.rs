use openssl::{
    bn::BigNum,
    encrypt::{Decrypter, Encrypter},
    pkey::{HasPublic, PKey},
    rsa::{Padding, Rsa},
    sign::{RsaPssSaltlen, Signer, Verifier},
};
use zeroize::Zeroizing;

use crate::jwk::{
    RsaJsonWebKeyExport, RsaJsonWebKeyImport, decode_jwk_base64url, encode_jwk_base64url,
    jwk_key_ops_allow_usages,
};
use crate::limits::{
    ensure_der_key_bytes, ensure_rsa_oaep_label_bytes, ensure_signature_operation_bytes,
};
use crate::{WebCryptoError, WebCryptoHashAlgorithm, WebCryptoKeyAlgorithm};

// Match the renderer-facing product policy for RSA key creation and import.
// The lower bound follows the WebCrypto boundary used locally; the upper bound
// prevents oversized imported keys from later making private operations stall
// the renderer while WebCrypto still runs synchronously.
pub const MIN_RSA_MODULUS_LENGTH_BITS: usize = 1024;
pub const MAX_RSA_MODULUS_LENGTH_BITS: usize = 16_384;
pub const MAX_RSA_PUBLIC_EXPONENT_BYTES: usize = 8;

pub struct RsaKeyPair {
    pub private_key: Zeroizing<Vec<u8>>,
    pub public_key: Vec<u8>,
    pub modulus_length_bits: usize,
    pub public_exponent: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RsaPublicKey {
    pub key_bytes: Vec<u8>,
    pub modulus_length_bits: usize,
    pub public_exponent: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RsaPrivateKey {
    pub key_bytes: Zeroizing<Vec<u8>>,
    pub modulus_length_bits: usize,
    pub public_exponent: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RsaImportedKey {
    Public(RsaPublicKey),
    Private(RsaPrivateKey),
}

pub fn generate_rsa_key_pair(
    modulus_length_bits: usize,
    public_exponent: &[u8],
) -> Result<RsaKeyPair, WebCryptoError> {
    ensure_rsa_modulus_length_bits(modulus_length_bits)?;
    ensure_rsa_public_exponent_bytes(public_exponent)?;
    let exponent = BigNum::from_slice(public_exponent).map_err(|_| WebCryptoError::Operation)?;
    let rsa = Rsa::generate_with_e(
        modulus_length_bits
            .try_into()
            .map_err(|_| WebCryptoError::Operation)?,
        &exponent,
    )
    .map_err(|_| WebCryptoError::Operation)?;
    let key = PKey::from_rsa(rsa).map_err(|_| WebCryptoError::Operation)?;
    let metadata = rsa_metadata_from_key(&key).map_err(|_| WebCryptoError::Operation)?;
    let public_key = key
        .public_key_to_der()
        .map_err(|_| WebCryptoError::Operation)?;
    let private_key = Zeroizing::new(
        key.private_key_to_pkcs8()
            .map_err(|_| WebCryptoError::Operation)?,
    );
    Ok(RsaKeyPair {
        private_key,
        public_key,
        modulus_length_bits: metadata.modulus_length_bits,
        public_exponent: metadata.public_exponent,
    })
}

pub fn import_rsa_spki_public_key(bytes: &[u8]) -> Result<RsaPublicKey, WebCryptoError> {
    ensure_der_key_bytes(bytes)?;
    let key = PKey::public_key_from_der(bytes).map_err(|_| WebCryptoError::Data)?;
    let metadata = rsa_import_metadata_from_key(&key)?;
    Ok(RsaPublicKey {
        key_bytes: key.public_key_to_der().map_err(|_| WebCryptoError::Data)?,
        modulus_length_bits: metadata.modulus_length_bits,
        public_exponent: metadata.public_exponent,
    })
}

pub fn import_rsa_pkcs8_private_key(bytes: &[u8]) -> Result<RsaPrivateKey, WebCryptoError> {
    ensure_der_key_bytes(bytes)?;
    let key = PKey::private_key_from_der(bytes).map_err(|_| WebCryptoError::Data)?;
    let metadata = rsa_import_metadata_from_key(&key)?;
    Ok(RsaPrivateKey {
        key_bytes: Zeroizing::new(
            key.private_key_to_pkcs8()
                .map_err(|_| WebCryptoError::Data)?,
        ),
        modulus_length_bits: metadata.modulus_length_bits,
        public_exponent: metadata.public_exponent,
    })
}

pub fn rsa_public_key_from_private(private_key_der: &[u8]) -> Result<Vec<u8>, WebCryptoError> {
    ensure_der_key_bytes(private_key_der)?;
    let key = PKey::private_key_from_der(private_key_der).map_err(|_| WebCryptoError::Operation)?;
    key.public_key_to_der()
        .map_err(|_| WebCryptoError::Operation)
}

pub fn import_rsa_jwk_key(
    jwk: &RsaJsonWebKeyImport,
    algorithm: WebCryptoKeyAlgorithm,
    hash: WebCryptoHashAlgorithm,
    extractable: bool,
    usages: &[String],
) -> Result<RsaImportedKey, WebCryptoError> {
    jwk.validate_resource_limits()?;
    if jwk.kty.as_deref() != Some("RSA") {
        return Err(WebCryptoError::Data);
    }
    if extractable && jwk.ext == Some(false) {
        return Err(WebCryptoError::Data);
    }
    if jwk_rsa_alg(jwk.alg.as_deref()).is_some_and(|expected| expected != (algorithm, hash))
        || jwk
            .alg
            .as_deref()
            .is_some_and(|alg| jwk_rsa_alg(Some(alg)).is_none())
    {
        return Err(WebCryptoError::Data);
    }
    jwk_key_ops_allow_usages(
        jwk.key_ops.as_deref(),
        usages,
        jwk.public_key_use.as_deref(),
    )?;

    let n = required_jwk_big_num(jwk.n.as_deref())?;
    let e = required_jwk_big_num(jwk.e.as_deref())?;
    ensure_rsa_jwk_public_component_import_bounds(&n, &e)?;
    if let Some(d) = jwk.d.as_deref() {
        let d = decode_jwk_big_num(d)?;
        let p = required_jwk_big_num(jwk.p.as_deref())?;
        let q = required_jwk_big_num(jwk.q.as_deref())?;
        let dp = required_jwk_big_num(jwk.dp.as_deref())?;
        let dq = required_jwk_big_num(jwk.dq.as_deref())?;
        let qi = required_jwk_big_num(jwk.qi.as_deref())?;
        ensure_rsa_jwk_private_component_import_bounds(&[&d, &p, &q, &dp, &dq, &qi])?;
        let rsa = Rsa::from_private_components(n, e, d, p, q, dp, dq, qi)
            .map_err(|_| WebCryptoError::Data)?;
        let key = PKey::from_rsa(rsa).map_err(|_| WebCryptoError::Data)?;
        let metadata = rsa_import_metadata_from_key(&key)?;
        return Ok(RsaImportedKey::Private(RsaPrivateKey {
            key_bytes: Zeroizing::new(
                key.private_key_to_pkcs8()
                    .map_err(|_| WebCryptoError::Data)?,
            ),
            modulus_length_bits: metadata.modulus_length_bits,
            public_exponent: metadata.public_exponent,
        }));
    }

    let rsa = Rsa::from_public_components(n, e).map_err(|_| WebCryptoError::Data)?;
    let key = PKey::from_rsa(rsa).map_err(|_| WebCryptoError::Data)?;
    let metadata = rsa_import_metadata_from_key(&key)?;
    Ok(RsaImportedKey::Public(RsaPublicKey {
        key_bytes: key.public_key_to_der().map_err(|_| WebCryptoError::Data)?,
        modulus_length_bits: metadata.modulus_length_bits,
        public_exponent: metadata.public_exponent,
    }))
}

pub fn export_rsa_jwk_public_key(
    spki_der: &[u8],
    algorithm: WebCryptoKeyAlgorithm,
    hash: WebCryptoHashAlgorithm,
    key_ops: Vec<String>,
    ext: bool,
) -> Result<RsaJsonWebKeyExport, WebCryptoError> {
    let key = PKey::public_key_from_der(spki_der).map_err(|_| WebCryptoError::Operation)?;
    let rsa = key.rsa().map_err(|_| WebCryptoError::Operation)?;
    Ok(RsaJsonWebKeyExport {
        kty: "RSA",
        n: encode_jwk_base64url(&rsa.n().to_vec()),
        e: encode_jwk_base64url(&rsa.e().to_vec()),
        d: None,
        p: None,
        q: None,
        dp: None,
        dq: None,
        qi: None,
        alg: rsa_jwk_alg(algorithm, hash).ok_or(WebCryptoError::Operation)?,
        key_ops,
        ext,
    })
}

pub fn export_rsa_jwk_private_key(
    pkcs8_der: &[u8],
    algorithm: WebCryptoKeyAlgorithm,
    hash: WebCryptoHashAlgorithm,
    key_ops: Vec<String>,
    ext: bool,
) -> Result<RsaJsonWebKeyExport, WebCryptoError> {
    let key = PKey::private_key_from_der(pkcs8_der).map_err(|_| WebCryptoError::Operation)?;
    let rsa = key.rsa().map_err(|_| WebCryptoError::Operation)?;
    Ok(RsaJsonWebKeyExport {
        kty: "RSA",
        n: encode_jwk_base64url(&rsa.n().to_vec()),
        e: encode_jwk_base64url(&rsa.e().to_vec()),
        d: Some(encode_jwk_base64url(&rsa.d().to_vec())),
        p: rsa.p().map(|value| encode_jwk_base64url(&value.to_vec())),
        q: rsa.q().map(|value| encode_jwk_base64url(&value.to_vec())),
        dp: rsa
            .dmp1()
            .map(|value| encode_jwk_base64url(&value.to_vec())),
        dq: rsa
            .dmq1()
            .map(|value| encode_jwk_base64url(&value.to_vec())),
        qi: rsa
            .iqmp()
            .map(|value| encode_jwk_base64url(&value.to_vec())),
        alg: rsa_jwk_alg(algorithm, hash).ok_or(WebCryptoError::Operation)?,
        key_ops,
        ext,
    })
}

pub fn rsa_oaep_encrypt(
    public_key_der: &[u8],
    hash: WebCryptoHashAlgorithm,
    label: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, WebCryptoError> {
    ensure_rsa_oaep_label_bytes(label)?;
    ensure_signature_operation_bytes(data)?;
    let key = PKey::public_key_from_der(public_key_der).map_err(|_| WebCryptoError::Operation)?;
    let mut encrypter = Encrypter::new(&key).map_err(|_| WebCryptoError::Operation)?;
    configure_oaep_encrypter(&mut encrypter, hash, label)?;
    let mut out = vec![
        0;
        encrypter
            .encrypt_len(data)
            .map_err(|_| WebCryptoError::Operation)?
    ];
    let len = encrypter
        .encrypt(data, &mut out)
        .map_err(|_| WebCryptoError::Operation)?;
    out.truncate(len);
    Ok(out)
}

pub fn rsa_oaep_decrypt(
    private_key_der: &[u8],
    hash: WebCryptoHashAlgorithm,
    label: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, WebCryptoError> {
    ensure_rsa_oaep_label_bytes(label)?;
    ensure_signature_operation_bytes(data)?;
    let key = PKey::private_key_from_der(private_key_der).map_err(|_| WebCryptoError::Operation)?;
    let mut decrypter = Decrypter::new(&key).map_err(|_| WebCryptoError::Operation)?;
    configure_oaep_decrypter(&mut decrypter, hash, label)?;
    let mut out = vec![
        0;
        decrypter
            .decrypt_len(data)
            .map_err(|_| WebCryptoError::Operation)?
    ];
    let len = decrypter
        .decrypt(data, &mut out)
        .map_err(|_| WebCryptoError::Operation)?;
    out.truncate(len);
    Ok(out)
}

pub fn rsa_pkcs1_sign(
    private_key_der: &[u8],
    hash: WebCryptoHashAlgorithm,
    data: &[u8],
) -> Result<Vec<u8>, WebCryptoError> {
    ensure_signature_operation_bytes(data)?;
    let key = PKey::private_key_from_der(private_key_der).map_err(|_| WebCryptoError::Operation)?;
    let mut signer =
        Signer::new(hash.message_digest(), &key).map_err(|_| WebCryptoError::Operation)?;
    signer
        .set_rsa_padding(Padding::PKCS1)
        .map_err(|_| WebCryptoError::Operation)?;
    signer.update(data).map_err(|_| WebCryptoError::Operation)?;
    signer.sign_to_vec().map_err(|_| WebCryptoError::Operation)
}

pub fn rsa_pkcs1_verify(
    public_key_der: &[u8],
    hash: WebCryptoHashAlgorithm,
    data: &[u8],
    signature: &[u8],
) -> Result<bool, WebCryptoError> {
    ensure_signature_operation_bytes(data)?;
    ensure_signature_operation_bytes(signature)?;
    let key = PKey::public_key_from_der(public_key_der).map_err(|_| WebCryptoError::Operation)?;
    let mut verifier =
        Verifier::new(hash.message_digest(), &key).map_err(|_| WebCryptoError::Operation)?;
    verifier
        .set_rsa_padding(Padding::PKCS1)
        .map_err(|_| WebCryptoError::Operation)?;
    verifier
        .update(data)
        .map_err(|_| WebCryptoError::Operation)?;
    verifier
        .verify(signature)
        .map_err(|_| WebCryptoError::Operation)
}

pub fn rsa_pss_sign(
    private_key_der: &[u8],
    hash: WebCryptoHashAlgorithm,
    salt_length: usize,
    data: &[u8],
) -> Result<Vec<u8>, WebCryptoError> {
    ensure_signature_operation_bytes(data)?;
    let key = PKey::private_key_from_der(private_key_der).map_err(|_| WebCryptoError::Operation)?;
    let mut signer =
        Signer::new(hash.message_digest(), &key).map_err(|_| WebCryptoError::Operation)?;
    configure_pss_signer(&mut signer, hash, salt_length)?;
    signer.update(data).map_err(|_| WebCryptoError::Operation)?;
    signer.sign_to_vec().map_err(|_| WebCryptoError::Operation)
}

pub fn rsa_pss_verify(
    public_key_der: &[u8],
    hash: WebCryptoHashAlgorithm,
    salt_length: usize,
    data: &[u8],
    signature: &[u8],
) -> Result<bool, WebCryptoError> {
    ensure_signature_operation_bytes(data)?;
    ensure_signature_operation_bytes(signature)?;
    let key = PKey::public_key_from_der(public_key_der).map_err(|_| WebCryptoError::Operation)?;
    let mut verifier =
        Verifier::new(hash.message_digest(), &key).map_err(|_| WebCryptoError::Operation)?;
    configure_pss_verifier(&mut verifier, hash, salt_length)?;
    verifier
        .update(data)
        .map_err(|_| WebCryptoError::Operation)?;
    verifier
        .verify(signature)
        .map_err(|_| WebCryptoError::Operation)
}

fn configure_oaep_encrypter(
    encrypter: &mut Encrypter<'_>,
    hash: WebCryptoHashAlgorithm,
    label: &[u8],
) -> Result<(), WebCryptoError> {
    encrypter
        .set_rsa_padding(Padding::PKCS1_OAEP)
        .map_err(|_| WebCryptoError::Operation)?;
    encrypter
        .set_rsa_oaep_md(hash.message_digest())
        .map_err(|_| WebCryptoError::Operation)?;
    encrypter
        .set_rsa_mgf1_md(hash.message_digest())
        .map_err(|_| WebCryptoError::Operation)?;
    if !label.is_empty() {
        encrypter
            .set_rsa_oaep_label(label)
            .map_err(|_| WebCryptoError::Operation)?;
    }
    Ok(())
}

fn configure_oaep_decrypter(
    decrypter: &mut Decrypter<'_>,
    hash: WebCryptoHashAlgorithm,
    label: &[u8],
) -> Result<(), WebCryptoError> {
    decrypter
        .set_rsa_padding(Padding::PKCS1_OAEP)
        .map_err(|_| WebCryptoError::Operation)?;
    decrypter
        .set_rsa_oaep_md(hash.message_digest())
        .map_err(|_| WebCryptoError::Operation)?;
    decrypter
        .set_rsa_mgf1_md(hash.message_digest())
        .map_err(|_| WebCryptoError::Operation)?;
    if !label.is_empty() {
        decrypter
            .set_rsa_oaep_label(label)
            .map_err(|_| WebCryptoError::Operation)?;
    }
    Ok(())
}

fn configure_pss_signer(
    signer: &mut Signer<'_>,
    hash: WebCryptoHashAlgorithm,
    salt_length: usize,
) -> Result<(), WebCryptoError> {
    signer
        .set_rsa_padding(Padding::PKCS1_PSS)
        .map_err(|_| WebCryptoError::Operation)?;
    signer
        .set_rsa_mgf1_md(hash.message_digest())
        .map_err(|_| WebCryptoError::Operation)?;
    signer
        .set_rsa_pss_saltlen(RsaPssSaltlen::custom(
            salt_length
                .try_into()
                .map_err(|_| WebCryptoError::Operation)?,
        ))
        .map_err(|_| WebCryptoError::Operation)
}

fn configure_pss_verifier(
    verifier: &mut Verifier<'_>,
    hash: WebCryptoHashAlgorithm,
    salt_length: usize,
) -> Result<(), WebCryptoError> {
    verifier
        .set_rsa_padding(Padding::PKCS1_PSS)
        .map_err(|_| WebCryptoError::Operation)?;
    verifier
        .set_rsa_mgf1_md(hash.message_digest())
        .map_err(|_| WebCryptoError::Operation)?;
    verifier
        .set_rsa_pss_saltlen(RsaPssSaltlen::custom(
            salt_length
                .try_into()
                .map_err(|_| WebCryptoError::Operation)?,
        ))
        .map_err(|_| WebCryptoError::Operation)
}

struct RsaMetadata {
    modulus_length_bits: usize,
    public_exponent: Vec<u8>,
}

fn rsa_metadata_from_key<T>(key: &PKey<T>) -> Result<RsaMetadata, WebCryptoError>
where
    T: HasPublic,
{
    let rsa = key.rsa().map_err(|_| WebCryptoError::Data)?;
    let metadata = RsaMetadata {
        modulus_length_bits: rsa.n().num_bits() as usize,
        public_exponent: rsa.e().to_vec(),
    };
    ensure_rsa_modulus_length_bits(metadata.modulus_length_bits)?;
    ensure_rsa_public_exponent_bytes(&metadata.public_exponent)?;
    Ok(metadata)
}

fn rsa_import_metadata_from_key<T>(key: &PKey<T>) -> Result<RsaMetadata, WebCryptoError>
where
    T: HasPublic,
{
    // Import parses caller-provided key material. Once DER/JWK container size
    // guards have passed, RSA metadata bounds failures are data conformance
    // errors rather than runtime operation failures.
    rsa_metadata_from_key(key).map_err(|_| WebCryptoError::Data)
}

fn required_jwk_big_num(value: Option<&str>) -> Result<BigNum, WebCryptoError> {
    value
        .map(decode_jwk_big_num)
        .unwrap_or(Err(WebCryptoError::Data))
}

fn decode_jwk_big_num(value: &str) -> Result<BigNum, WebCryptoError> {
    BigNum::from_slice(&decode_jwk_base64url(value)?).map_err(|_| WebCryptoError::Data)
}

fn ensure_rsa_jwk_public_component_import_bounds(
    modulus: &BigNum,
    public_exponent: &BigNum,
) -> Result<(), WebCryptoError> {
    if modulus.num_bits() as usize > MAX_RSA_MODULUS_LENGTH_BITS
        || public_exponent.num_bits() as usize > MAX_RSA_PUBLIC_EXPONENT_BYTES * 8
    {
        Err(WebCryptoError::Data)
    } else {
        Ok(())
    }
}

fn ensure_rsa_jwk_private_component_import_bounds(
    components: &[&BigNum],
) -> Result<(), WebCryptoError> {
    if components
        .iter()
        .any(|component| component.num_bits() as usize > MAX_RSA_MODULUS_LENGTH_BITS)
    {
        Err(WebCryptoError::Data)
    } else {
        Ok(())
    }
}

fn ensure_rsa_modulus_length_bits(length_bits: usize) -> Result<(), WebCryptoError> {
    if (MIN_RSA_MODULUS_LENGTH_BITS..=MAX_RSA_MODULUS_LENGTH_BITS).contains(&length_bits) {
        Ok(())
    } else {
        Err(WebCryptoError::Operation)
    }
}

fn ensure_rsa_public_exponent_bytes(bytes: &[u8]) -> Result<(), WebCryptoError> {
    if bytes.is_empty() || bytes.len() > MAX_RSA_PUBLIC_EXPONENT_BYTES {
        Err(WebCryptoError::Operation)
    } else {
        Ok(())
    }
}

fn jwk_rsa_alg(alg: Option<&str>) -> Option<(WebCryptoKeyAlgorithm, WebCryptoHashAlgorithm)> {
    Some(match alg? {
        "RSA-OAEP" => (WebCryptoKeyAlgorithm::RsaOaep, WebCryptoHashAlgorithm::Sha1),
        "RSA-OAEP-256" => (
            WebCryptoKeyAlgorithm::RsaOaep,
            WebCryptoHashAlgorithm::Sha256,
        ),
        "RSA-OAEP-384" => (
            WebCryptoKeyAlgorithm::RsaOaep,
            WebCryptoHashAlgorithm::Sha384,
        ),
        "RSA-OAEP-512" => (
            WebCryptoKeyAlgorithm::RsaOaep,
            WebCryptoHashAlgorithm::Sha512,
        ),
        "RS1" => (
            WebCryptoKeyAlgorithm::RsassaPkcs1V15,
            WebCryptoHashAlgorithm::Sha1,
        ),
        "RS256" => (
            WebCryptoKeyAlgorithm::RsassaPkcs1V15,
            WebCryptoHashAlgorithm::Sha256,
        ),
        "RS384" => (
            WebCryptoKeyAlgorithm::RsassaPkcs1V15,
            WebCryptoHashAlgorithm::Sha384,
        ),
        "RS512" => (
            WebCryptoKeyAlgorithm::RsassaPkcs1V15,
            WebCryptoHashAlgorithm::Sha512,
        ),
        "PS1" => (WebCryptoKeyAlgorithm::RsaPss, WebCryptoHashAlgorithm::Sha1),
        "PS256" => (
            WebCryptoKeyAlgorithm::RsaPss,
            WebCryptoHashAlgorithm::Sha256,
        ),
        "PS384" => (
            WebCryptoKeyAlgorithm::RsaPss,
            WebCryptoHashAlgorithm::Sha384,
        ),
        "PS512" => (
            WebCryptoKeyAlgorithm::RsaPss,
            WebCryptoHashAlgorithm::Sha512,
        ),
        _ => return None,
    })
}

fn rsa_jwk_alg(
    algorithm: WebCryptoKeyAlgorithm,
    hash: WebCryptoHashAlgorithm,
) -> Option<&'static str> {
    Some(match (algorithm, hash) {
        (WebCryptoKeyAlgorithm::RsaOaep, WebCryptoHashAlgorithm::Sha1) => "RSA-OAEP",
        (WebCryptoKeyAlgorithm::RsaOaep, WebCryptoHashAlgorithm::Sha256) => "RSA-OAEP-256",
        (WebCryptoKeyAlgorithm::RsaOaep, WebCryptoHashAlgorithm::Sha384) => "RSA-OAEP-384",
        (WebCryptoKeyAlgorithm::RsaOaep, WebCryptoHashAlgorithm::Sha512) => "RSA-OAEP-512",
        (WebCryptoKeyAlgorithm::RsassaPkcs1V15, WebCryptoHashAlgorithm::Sha1) => "RS1",
        (WebCryptoKeyAlgorithm::RsassaPkcs1V15, WebCryptoHashAlgorithm::Sha256) => "RS256",
        (WebCryptoKeyAlgorithm::RsassaPkcs1V15, WebCryptoHashAlgorithm::Sha384) => "RS384",
        (WebCryptoKeyAlgorithm::RsassaPkcs1V15, WebCryptoHashAlgorithm::Sha512) => "RS512",
        (WebCryptoKeyAlgorithm::RsaPss, WebCryptoHashAlgorithm::Sha1) => "PS1",
        (WebCryptoKeyAlgorithm::RsaPss, WebCryptoHashAlgorithm::Sha256) => "PS256",
        (WebCryptoKeyAlgorithm::RsaPss, WebCryptoHashAlgorithm::Sha384) => "PS384",
        (WebCryptoKeyAlgorithm::RsaPss, WebCryptoHashAlgorithm::Sha512) => "PS512",
        _ => return None,
    })
}
