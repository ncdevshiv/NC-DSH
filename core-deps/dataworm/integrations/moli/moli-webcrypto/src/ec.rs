use openssl::{
    bn::{BigNum, BigNumContext},
    derive::Deriver,
    ec::{EcGroup, EcKey, EcPoint, PointConversionForm},
    ecdsa::EcdsaSig,
    nid::Nid,
    pkey::{HasParams, HasPublic, PKey, Private},
};
use zeroize::Zeroizing;

use crate::bits::truncate_derived_bits;
use crate::jwk::{
    EcJsonWebKeyExport, EcJsonWebKeyImport, decode_jwk_base64url, encode_jwk_base64url,
    jwk_key_ops_allow_usages,
};
use crate::limits::{ensure_der_key_bytes, ensure_signature_operation_bytes};
use crate::{WebCryptoError, WebCryptoHashAlgorithm, WebCryptoKeyAlgorithm};

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::AsRefStr, strum::EnumString)]
pub enum WebCryptoEcNamedCurve {
    #[strum(serialize = "P-256")]
    P256,
    #[strum(serialize = "P-384")]
    P384,
    #[strum(serialize = "P-521")]
    P521,
}

impl WebCryptoEcNamedCurve {
    pub fn name(self) -> &'static str {
        match self {
            Self::P256 => "P-256",
            Self::P384 => "P-384",
            Self::P521 => "P-521",
        }
    }

    pub fn coordinate_len_bytes(self) -> usize {
        match self {
            Self::P256 => 32,
            Self::P384 => 48,
            Self::P521 => 66,
        }
    }

    fn nid(self) -> Nid {
        match self {
            Self::P256 => Nid::X9_62_PRIME256V1,
            Self::P384 => Nid::SECP384R1,
            Self::P521 => Nid::SECP521R1,
        }
    }

    fn from_nid(nid: Nid) -> Option<Self> {
        match nid {
            Nid::X9_62_PRIME256V1 => Some(Self::P256),
            Nid::SECP384R1 => Some(Self::P384),
            Nid::SECP521R1 => Some(Self::P521),
            _ => None,
        }
    }
}

pub struct EcKeyPair {
    pub private_key: Zeroizing<Vec<u8>>,
    pub public_key: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcPublicKey {
    pub key_bytes: Vec<u8>,
    pub curve: WebCryptoEcNamedCurve,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcPrivateKey {
    pub key_bytes: Zeroizing<Vec<u8>>,
    pub curve: WebCryptoEcNamedCurve,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EcImportedKey {
    Public(EcPublicKey),
    Private(EcPrivateKey),
}

pub fn generate_ec_key_pair(curve: WebCryptoEcNamedCurve) -> Result<EcKeyPair, WebCryptoError> {
    let group = EcGroup::from_curve_name(curve.nid()).map_err(|_| WebCryptoError::Operation)?;
    let ec_key = EcKey::generate(&group).map_err(|_| WebCryptoError::Operation)?;
    let pkey = PKey::from_ec_key(ec_key).map_err(|_| WebCryptoError::Operation)?;
    Ok(EcKeyPair {
        private_key: Zeroizing::new(
            pkey.private_key_to_pkcs8()
                .map_err(|_| WebCryptoError::Operation)?,
        ),
        public_key: pkey
            .public_key_to_der()
            .map_err(|_| WebCryptoError::Operation)?,
    })
}

pub fn import_ec_raw_public_key(
    bytes: &[u8],
    curve: WebCryptoEcNamedCurve,
) -> Result<EcPublicKey, WebCryptoError> {
    let group = EcGroup::from_curve_name(curve.nid()).map_err(|_| WebCryptoError::Data)?;
    let mut ctx = BigNumContext::new().map_err(|_| WebCryptoError::Data)?;
    let point = EcPoint::from_bytes(&group, bytes, &mut ctx).map_err(|_| WebCryptoError::Data)?;
    let ec_key = EcKey::from_public_key(&group, &point).map_err(|_| WebCryptoError::Data)?;
    ec_key.check_key().map_err(|_| WebCryptoError::Data)?;
    let key = PKey::from_ec_key(ec_key).map_err(|_| WebCryptoError::Data)?;
    Ok(EcPublicKey {
        key_bytes: ec_public_key_to_uncompressed_der(&key).map_err(|_| WebCryptoError::Data)?,
        curve,
    })
}

pub fn export_ec_raw_public_key(spki_der: &[u8]) -> Result<Vec<u8>, WebCryptoError> {
    let key = PKey::public_key_from_der(spki_der).map_err(|_| WebCryptoError::Operation)?;
    let ec_key = key.ec_key().map_err(|_| WebCryptoError::Operation)?;
    let mut ctx = BigNumContext::new().map_err(|_| WebCryptoError::Operation)?;
    ec_key
        .public_key()
        .to_bytes(ec_key.group(), PointConversionForm::UNCOMPRESSED, &mut ctx)
        .map_err(|_| WebCryptoError::Operation)
}

pub fn import_ec_spki_public_key(
    bytes: &[u8],
    expected_curve: WebCryptoEcNamedCurve,
) -> Result<EcPublicKey, WebCryptoError> {
    ensure_der_key_bytes(bytes)?;
    let key = PKey::public_key_from_der(bytes).map_err(|_| WebCryptoError::Data)?;
    let curve = ec_curve_from_key(&key).map_err(|_| WebCryptoError::Data)?;
    if curve != expected_curve {
        return Err(WebCryptoError::Data);
    }
    Ok(EcPublicKey {
        key_bytes: ec_public_key_to_uncompressed_der(&key).map_err(|_| WebCryptoError::Data)?,
        curve,
    })
}

pub fn import_ec_pkcs8_private_key(
    bytes: &[u8],
    expected_curve: WebCryptoEcNamedCurve,
) -> Result<EcPrivateKey, WebCryptoError> {
    ensure_der_key_bytes(bytes)?;
    let key = PKey::private_key_from_der(bytes).map_err(|_| WebCryptoError::Data)?;
    let curve = ec_curve_from_key(&key).map_err(|_| WebCryptoError::Data)?;
    if curve != expected_curve {
        return Err(WebCryptoError::Data);
    }
    Ok(EcPrivateKey {
        key_bytes: Zeroizing::new(
            key.private_key_to_pkcs8()
                .map_err(|_| WebCryptoError::Data)?,
        ),
        curve,
    })
}

pub fn ec_public_key_from_private(
    private_key_der: &[u8],
    expected_curve: WebCryptoEcNamedCurve,
) -> Result<Vec<u8>, WebCryptoError> {
    ensure_der_key_bytes(private_key_der)?;
    let key = PKey::private_key_from_der(private_key_der).map_err(|_| WebCryptoError::Operation)?;
    if ec_curve_from_key(&key)? != expected_curve {
        return Err(WebCryptoError::Operation);
    }
    ec_public_key_to_uncompressed_der(&key)
}

pub fn import_ec_jwk_key(
    jwk: &EcJsonWebKeyImport,
    algorithm: WebCryptoKeyAlgorithm,
    expected_curve: WebCryptoEcNamedCurve,
    extractable: bool,
    usages: &[String],
) -> Result<EcImportedKey, WebCryptoError> {
    jwk.validate_resource_limits()?;
    if jwk.kty.as_deref() != Some("EC") || jwk.crv.as_deref() != Some(expected_curve.as_ref()) {
        return Err(WebCryptoError::Data);
    }
    if extractable && jwk.ext == Some(false) {
        return Err(WebCryptoError::Data);
    }
    if algorithm == WebCryptoKeyAlgorithm::Ecdsa
        && jwk
            .alg
            .as_deref()
            .is_some_and(|alg| ec_jwk_alg(expected_curve) != Some(alg))
    {
        return Err(WebCryptoError::Data);
    }
    jwk_key_ops_allow_usages(
        jwk.key_ops.as_deref(),
        usages,
        jwk.public_key_use.as_deref(),
    )?;

    let group = EcGroup::from_curve_name(expected_curve.nid()).map_err(|_| WebCryptoError::Data)?;
    let x = required_ec_jwk_coordinate(jwk.x.as_deref(), expected_curve)?;
    let y = required_ec_jwk_coordinate(jwk.y.as_deref(), expected_curve)?;
    let public_key = EcKey::from_public_key_affine_coordinates(&group, &x, &y)
        .map_err(|_| WebCryptoError::Data)?;
    public_key.check_key().map_err(|_| WebCryptoError::Data)?;
    if let Some(d) = jwk.d.as_deref() {
        let d = required_ec_jwk_coordinate(Some(d), expected_curve)?;
        let private_key = EcKey::from_private_components(&group, &d, public_key.public_key())
            .map_err(|_| WebCryptoError::Data)?;
        private_key.check_key().map_err(|_| WebCryptoError::Data)?;
        let pkey = PKey::from_ec_key(private_key).map_err(|_| WebCryptoError::Data)?;
        return Ok(EcImportedKey::Private(EcPrivateKey {
            key_bytes: Zeroizing::new(
                pkey.private_key_to_pkcs8()
                    .map_err(|_| WebCryptoError::Data)?,
            ),
            curve: expected_curve,
        }));
    }
    let pkey = PKey::from_ec_key(public_key).map_err(|_| WebCryptoError::Data)?;
    Ok(EcImportedKey::Public(EcPublicKey {
        key_bytes: ec_public_key_to_uncompressed_der(&pkey).map_err(|_| WebCryptoError::Data)?,
        curve: expected_curve,
    }))
}

pub fn export_ec_jwk_public_key(
    spki_der: &[u8],
    algorithm: WebCryptoKeyAlgorithm,
    key_ops: Vec<String>,
    ext: bool,
) -> Result<EcJsonWebKeyExport, WebCryptoError> {
    let key = PKey::public_key_from_der(spki_der).map_err(|_| WebCryptoError::Operation)?;
    let (curve, x, y) = ec_jwk_public_parts_from_key(&key)?;
    Ok(EcJsonWebKeyExport {
        kty: "EC",
        crv: curve.name(),
        x,
        y,
        d: None,
        alg: (algorithm == WebCryptoKeyAlgorithm::Ecdsa)
            .then(|| ec_jwk_alg(curve))
            .flatten(),
        key_ops,
        ext,
    })
}

pub fn export_ec_jwk_private_key(
    pkcs8_der: &[u8],
    algorithm: WebCryptoKeyAlgorithm,
    key_ops: Vec<String>,
    ext: bool,
) -> Result<EcJsonWebKeyExport, WebCryptoError> {
    let key = PKey::private_key_from_der(pkcs8_der).map_err(|_| WebCryptoError::Operation)?;
    let (curve, x, y, d) = ec_jwk_private_parts_from_key(&key)?;
    Ok(EcJsonWebKeyExport {
        kty: "EC",
        crv: curve.name(),
        x,
        y,
        d,
        alg: (algorithm == WebCryptoKeyAlgorithm::Ecdsa)
            .then(|| ec_jwk_alg(curve))
            .flatten(),
        key_ops,
        ext,
    })
}

pub fn ecdsa_sign(
    private_key_der: &[u8],
    curve: WebCryptoEcNamedCurve,
    hash: WebCryptoHashAlgorithm,
    data: &[u8],
) -> Result<Vec<u8>, WebCryptoError> {
    ensure_signature_operation_bytes(data)?;
    let key = PKey::private_key_from_der(private_key_der).map_err(|_| WebCryptoError::Operation)?;
    if ec_curve_from_key(&key)? != curve {
        return Err(WebCryptoError::Operation);
    }
    let ec_key = key.ec_key().map_err(|_| WebCryptoError::Operation)?;
    let digest = hash.digest(data)?;
    let signature = EcdsaSig::sign(&digest, &ec_key).map_err(|_| WebCryptoError::Operation)?;
    let len = curve.coordinate_len_bytes() as i32;
    let mut raw = Vec::with_capacity(curve.coordinate_len_bytes() * 2);
    raw.extend_from_slice(
        &signature
            .r()
            .to_vec_padded(len)
            .map_err(|_| WebCryptoError::Operation)?,
    );
    raw.extend_from_slice(
        &signature
            .s()
            .to_vec_padded(len)
            .map_err(|_| WebCryptoError::Operation)?,
    );
    Ok(raw)
}

pub fn ecdsa_verify(
    public_key_der: &[u8],
    curve: WebCryptoEcNamedCurve,
    hash: WebCryptoHashAlgorithm,
    data: &[u8],
    signature: &[u8],
) -> Result<bool, WebCryptoError> {
    ensure_signature_operation_bytes(data)?;
    ensure_signature_operation_bytes(signature)?;
    let len = curve.coordinate_len_bytes();
    if signature.len() != len * 2 {
        return Ok(false);
    }
    let key = PKey::public_key_from_der(public_key_der).map_err(|_| WebCryptoError::Operation)?;
    if ec_curve_from_key(&key)? != curve {
        return Err(WebCryptoError::Operation);
    }
    let ec_key = key.ec_key().map_err(|_| WebCryptoError::Operation)?;
    let r = BigNum::from_slice(&signature[..len]).map_err(|_| WebCryptoError::Operation)?;
    let s = BigNum::from_slice(&signature[len..]).map_err(|_| WebCryptoError::Operation)?;
    let signature =
        EcdsaSig::from_private_components(r, s).map_err(|_| WebCryptoError::Operation)?;
    let digest = hash.digest(data)?;
    signature
        .verify(&digest, &ec_key)
        .map_err(|_| WebCryptoError::Operation)
}

pub fn derive_ecdh_bits(
    private_key_der: &[u8],
    public_key_der: &[u8],
    curve: WebCryptoEcNamedCurve,
    length_bits: usize,
) -> Result<Vec<u8>, WebCryptoError> {
    let private_key =
        PKey::private_key_from_der(private_key_der).map_err(|_| WebCryptoError::Operation)?;
    let public_key =
        PKey::public_key_from_der(public_key_der).map_err(|_| WebCryptoError::Operation)?;
    if ec_curve_from_key(&private_key)? != curve || ec_curve_from_key(&public_key)? != curve {
        return Err(WebCryptoError::Operation);
    }
    let mut deriver = Deriver::new(&private_key).map_err(|_| WebCryptoError::Operation)?;
    deriver
        .set_peer(&public_key)
        .map_err(|_| WebCryptoError::Operation)?;
    let secret = deriver
        .derive_to_vec()
        .map_err(|_| WebCryptoError::Operation)?;
    truncate_derived_bits(&secret, length_bits)
}

fn ec_curve_from_key<T>(key: &PKey<T>) -> Result<WebCryptoEcNamedCurve, WebCryptoError>
where
    T: HasParams + HasPublic,
{
    let ec_key = key.ec_key().map_err(|_| WebCryptoError::Data)?;
    let Some(nid) = ec_key.group().curve_name() else {
        return Err(WebCryptoError::Data);
    };
    WebCryptoEcNamedCurve::from_nid(nid).ok_or(WebCryptoError::Data)
}

fn ec_jwk_public_parts_from_key<T>(
    key: &PKey<T>,
) -> Result<(WebCryptoEcNamedCurve, String, String), WebCryptoError>
where
    T: HasParams + HasPublic,
{
    let ec_key = key.ec_key().map_err(|_| WebCryptoError::Operation)?;
    let Some(nid) = ec_key.group().curve_name() else {
        return Err(WebCryptoError::Operation);
    };
    let curve = WebCryptoEcNamedCurve::from_nid(nid).ok_or(WebCryptoError::Operation)?;
    let len = curve.coordinate_len_bytes() as i32;
    let mut ctx = BigNumContext::new().map_err(|_| WebCryptoError::Operation)?;
    let mut x = BigNum::new().map_err(|_| WebCryptoError::Operation)?;
    let mut y = BigNum::new().map_err(|_| WebCryptoError::Operation)?;
    ec_key
        .public_key()
        .affine_coordinates(ec_key.group(), &mut x, &mut y, &mut ctx)
        .map_err(|_| WebCryptoError::Operation)?;
    Ok((
        curve,
        encode_jwk_base64url(
            &x.to_vec_padded(len)
                .map_err(|_| WebCryptoError::Operation)?,
        ),
        encode_jwk_base64url(
            &y.to_vec_padded(len)
                .map_err(|_| WebCryptoError::Operation)?,
        ),
    ))
}

fn ec_public_key_to_uncompressed_der<T>(key: &PKey<T>) -> Result<Vec<u8>, WebCryptoError>
where
    T: HasPublic,
{
    let ec_key = key.ec_key().map_err(|_| WebCryptoError::Operation)?;
    let mut ctx = BigNumContext::new().map_err(|_| WebCryptoError::Operation)?;
    let raw = ec_key
        .public_key()
        .to_bytes(ec_key.group(), PointConversionForm::UNCOMPRESSED, &mut ctx)
        .map_err(|_| WebCryptoError::Operation)?;
    let point = EcPoint::from_bytes(ec_key.group(), &raw, &mut ctx)
        .map_err(|_| WebCryptoError::Operation)?;
    let public_key =
        EcKey::from_public_key(ec_key.group(), &point).map_err(|_| WebCryptoError::Operation)?;
    PKey::from_ec_key(public_key)
        .and_then(|key| key.public_key_to_der())
        .map_err(|_| WebCryptoError::Operation)
}

fn ec_jwk_private_parts_from_key(
    key: &PKey<Private>,
) -> Result<(WebCryptoEcNamedCurve, String, String, Option<String>), WebCryptoError> {
    let (curve, x, y) = ec_jwk_public_parts_from_key(key)?;
    let ec_key = key.ec_key().map_err(|_| WebCryptoError::Operation)?;
    let d = ec_key
        .private_key()
        .to_vec_padded(curve.coordinate_len_bytes() as i32)
        .map_err(|_| WebCryptoError::Operation)
        .map(|value| encode_jwk_base64url(&value))?;
    Ok((curve, x, y, Some(d)))
}

fn required_ec_jwk_coordinate(
    value: Option<&str>,
    curve: WebCryptoEcNamedCurve,
) -> Result<BigNum, WebCryptoError> {
    let Some(value) = value else {
        return Err(WebCryptoError::Data);
    };
    let bytes = decode_jwk_base64url(value)?;
    if bytes.len() != curve.coordinate_len_bytes() {
        return Err(WebCryptoError::Data);
    }
    BigNum::from_slice(&bytes).map_err(|_| WebCryptoError::Data)
}

fn ec_jwk_alg(curve: WebCryptoEcNamedCurve) -> Option<&'static str> {
    Some(match curve {
        WebCryptoEcNamedCurve::P256 => "ES256",
        WebCryptoEcNamedCurve::P384 => "ES384",
        WebCryptoEcNamedCurve::P521 => "ES512",
    })
}
