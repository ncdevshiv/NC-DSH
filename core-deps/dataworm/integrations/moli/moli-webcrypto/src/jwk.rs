use base64::{
    Engine,
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
};

use crate::WebCryptoError;

// Product bounds for page-controlled JsonWebKey data. These are intentionally
// much larger than the supported key sizes, but small enough to keep malformed
// JWK input from forcing large Rust allocations or base64url decoding work.
pub const MAX_JWK_MEMBER_BYTES: usize = 64 * 1024;
pub const MAX_JWK_SERIALIZED_BYTES: usize = 256 * 1024;
pub const MAX_JWK_KEY_OPS: usize = 64;

#[derive(serde::Deserialize)]
pub struct HmacJsonWebKeyImport {
    pub kty: Option<String>,
    pub k: Option<String>,
    pub alg: Option<String>,
    pub key_ops: Option<Vec<String>>,
    pub ext: Option<bool>,
    #[serde(rename = "use")]
    pub public_key_use: Option<String>,
}

#[derive(serde::Serialize)]
pub struct HmacJsonWebKeyExport {
    pub kty: &'static str,
    pub k: String,
    pub alg: &'static str,
    pub key_ops: Vec<String>,
    pub ext: bool,
}

#[derive(serde::Deserialize)]
pub struct AesJsonWebKeyImport {
    pub kty: Option<String>,
    pub k: Option<String>,
    pub alg: Option<String>,
    pub key_ops: Option<Vec<String>>,
    pub ext: Option<bool>,
    #[serde(rename = "use")]
    pub public_key_use: Option<String>,
}

#[derive(serde::Serialize)]
pub struct AesJsonWebKeyExport {
    pub kty: &'static str,
    pub k: String,
    pub alg: String,
    pub key_ops: Vec<String>,
    pub ext: bool,
}

#[derive(serde::Deserialize)]
pub struct Chacha20Poly1305JsonWebKeyImport {
    pub kty: Option<String>,
    pub k: Option<String>,
    pub alg: Option<String>,
    pub key_ops: Option<Vec<String>>,
    pub ext: Option<bool>,
    #[serde(rename = "use")]
    pub public_key_use: Option<String>,
}

#[derive(serde::Serialize)]
pub struct Chacha20Poly1305JsonWebKeyExport {
    pub kty: &'static str,
    pub k: String,
    pub key_ops: Vec<String>,
    pub ext: bool,
}

#[derive(serde::Deserialize)]
pub struct OkpJsonWebKeyImport {
    pub kty: Option<String>,
    pub crv: Option<String>,
    pub x: Option<String>,
    pub d: Option<String>,
    pub alg: Option<String>,
    pub key_ops: Option<Vec<String>>,
    pub ext: Option<bool>,
    #[serde(rename = "use")]
    pub public_key_use: Option<String>,
}

#[derive(serde::Serialize)]
pub struct OkpJsonWebKeyExport {
    pub kty: &'static str,
    pub crv: &'static str,
    pub x: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alg: Option<&'static str>,
    pub key_ops: Vec<String>,
    pub ext: bool,
}

#[derive(serde::Deserialize)]
pub struct RsaJsonWebKeyImport {
    pub kty: Option<String>,
    pub n: Option<String>,
    pub e: Option<String>,
    pub d: Option<String>,
    pub p: Option<String>,
    pub q: Option<String>,
    pub dp: Option<String>,
    pub dq: Option<String>,
    pub qi: Option<String>,
    pub alg: Option<String>,
    pub key_ops: Option<Vec<String>>,
    pub ext: Option<bool>,
    #[serde(rename = "use")]
    pub public_key_use: Option<String>,
}

#[derive(serde::Serialize)]
pub struct RsaJsonWebKeyExport {
    pub kty: &'static str,
    pub n: String,
    pub e: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dq: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qi: Option<String>,
    pub alg: &'static str,
    pub key_ops: Vec<String>,
    pub ext: bool,
}

#[derive(serde::Deserialize)]
pub struct EcJsonWebKeyImport {
    pub kty: Option<String>,
    pub crv: Option<String>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub d: Option<String>,
    pub alg: Option<String>,
    pub key_ops: Option<Vec<String>>,
    pub ext: Option<bool>,
    #[serde(rename = "use")]
    pub public_key_use: Option<String>,
}

#[derive(serde::Serialize)]
pub struct EcJsonWebKeyExport {
    pub kty: &'static str,
    pub crv: &'static str,
    pub x: String,
    pub y: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alg: Option<&'static str>,
    pub key_ops: Vec<String>,
    pub ext: bool,
}

impl HmacJsonWebKeyImport {
    pub fn validate_resource_limits(&self) -> Result<(), WebCryptoError> {
        validate_jwk_common_members(
            self.kty.as_deref(),
            self.public_key_use.as_deref(),
            self.alg.as_deref(),
            self.key_ops.as_deref(),
        )?;
        validate_jwk_optional_member(self.k.as_deref())
    }
}

impl AesJsonWebKeyImport {
    pub fn validate_resource_limits(&self) -> Result<(), WebCryptoError> {
        validate_jwk_common_members(
            self.kty.as_deref(),
            self.public_key_use.as_deref(),
            self.alg.as_deref(),
            self.key_ops.as_deref(),
        )?;
        validate_jwk_optional_member(self.k.as_deref())
    }
}

impl Chacha20Poly1305JsonWebKeyImport {
    pub fn validate_resource_limits(&self) -> Result<(), WebCryptoError> {
        validate_jwk_common_members(
            self.kty.as_deref(),
            self.public_key_use.as_deref(),
            self.alg.as_deref(),
            self.key_ops.as_deref(),
        )?;
        validate_jwk_optional_member(self.k.as_deref())
    }
}

impl OkpJsonWebKeyImport {
    pub fn validate_resource_limits(&self) -> Result<(), WebCryptoError> {
        validate_jwk_common_members(
            self.kty.as_deref(),
            self.public_key_use.as_deref(),
            self.alg.as_deref(),
            self.key_ops.as_deref(),
        )?;
        validate_jwk_optional_member(self.crv.as_deref())?;
        validate_jwk_optional_member(self.x.as_deref())?;
        validate_jwk_optional_member(self.d.as_deref())
    }
}

impl RsaJsonWebKeyImport {
    pub fn validate_resource_limits(&self) -> Result<(), WebCryptoError> {
        validate_jwk_common_members(
            self.kty.as_deref(),
            self.public_key_use.as_deref(),
            self.alg.as_deref(),
            self.key_ops.as_deref(),
        )?;
        for member in [
            self.n.as_deref(),
            self.e.as_deref(),
            self.d.as_deref(),
            self.p.as_deref(),
            self.q.as_deref(),
            self.dp.as_deref(),
            self.dq.as_deref(),
            self.qi.as_deref(),
        ] {
            validate_jwk_optional_member(member)?;
        }
        Ok(())
    }
}

impl EcJsonWebKeyImport {
    pub fn validate_resource_limits(&self) -> Result<(), WebCryptoError> {
        validate_jwk_common_members(
            self.kty.as_deref(),
            self.public_key_use.as_deref(),
            self.alg.as_deref(),
            self.key_ops.as_deref(),
        )?;
        validate_jwk_optional_member(self.crv.as_deref())?;
        validate_jwk_optional_member(self.x.as_deref())?;
        validate_jwk_optional_member(self.y.as_deref())?;
        validate_jwk_optional_member(self.d.as_deref())
    }
}

pub(crate) fn encode_jwk_base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn decode_jwk_base64url(value: &str) -> Result<Vec<u8>, WebCryptoError> {
    validate_jwk_member(value)?;
    URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .or_else(|_| URL_SAFE.decode(value.as_bytes()))
        .map_err(|_| WebCryptoError::Data)
}

pub(crate) fn jwk_use_allows_algorithm(
    public_key_use: Option<&str>,
    expected: &str,
) -> Result<(), WebCryptoError> {
    if let Some(public_key_use) = public_key_use
        && public_key_use != expected
    {
        return Err(WebCryptoError::Data);
    }
    Ok(())
}

pub(crate) fn jwk_key_ops_allow_usages(
    key_ops: Option<&[String]>,
    usages: &[String],
    public_key_use: Option<&str>,
) -> Result<(), WebCryptoError> {
    validate_jwk_key_ops(key_ops)?;
    let key_ops_mask = if let Some(key_ops) = key_ops {
        let mut mask = 0_u16;
        for (index, usage) in key_ops.iter().enumerate() {
            // Chromium ignores distinct unknown JWK key_ops, but still rejects
            // duplicate recognized or unknown entries.
            if key_ops[..index].iter().any(|existing| existing == usage) {
                return Err(WebCryptoError::Data);
            }
            if let Some(bit) = jwk_key_op_usage_bit(usage) {
                mask |= bit;
            }
        }
        jwk_requested_usages_fit_mask(usages, mask)?;
        Some(mask)
    } else {
        None
    };

    if let Some(public_key_use) = public_key_use {
        let Some(use_mask) = jwk_public_key_use_mask(public_key_use) else {
            return Err(WebCryptoError::Data);
        };
        jwk_requested_usages_fit_mask(usages, use_mask)?;
        if key_ops_mask.is_some_and(|mask| mask & !use_mask != 0) {
            return Err(WebCryptoError::Data);
        }
    }
    Ok(())
}

fn jwk_requested_usages_fit_mask(
    usages: &[String],
    allowed_mask: u16,
) -> Result<(), WebCryptoError> {
    for usage in usages {
        let Some(bit) = jwk_key_op_usage_bit(usage) else {
            return Err(WebCryptoError::Data);
        };
        if allowed_mask & bit == 0 {
            return Err(WebCryptoError::Data);
        }
    }
    Ok(())
}

fn jwk_key_op_usage_bit(usage: &str) -> Option<u16> {
    Some(match usage {
        "encrypt" => 1 << 0,
        "decrypt" => 1 << 1,
        "sign" => 1 << 2,
        "verify" => 1 << 3,
        "deriveKey" => 1 << 4,
        "deriveBits" => 1 << 5,
        "wrapKey" => 1 << 6,
        "unwrapKey" => 1 << 7,
        _ => return None,
    })
}

fn validate_jwk_common_members(
    kty: Option<&str>,
    public_key_use: Option<&str>,
    alg: Option<&str>,
    key_ops: Option<&[String]>,
) -> Result<(), WebCryptoError> {
    validate_jwk_optional_member(kty)?;
    validate_jwk_optional_member(public_key_use)?;
    validate_jwk_optional_member(alg)?;
    validate_jwk_key_ops(key_ops)
}

fn validate_jwk_optional_member(value: Option<&str>) -> Result<(), WebCryptoError> {
    match value {
        Some(value) => validate_jwk_member(value),
        None => Ok(()),
    }
}

fn validate_jwk_member(value: &str) -> Result<(), WebCryptoError> {
    if value.len() > MAX_JWK_MEMBER_BYTES {
        Err(WebCryptoError::Operation)
    } else {
        Ok(())
    }
}

fn validate_jwk_key_ops(key_ops: Option<&[String]>) -> Result<(), WebCryptoError> {
    let Some(key_ops) = key_ops else {
        return Ok(());
    };
    if key_ops.len() > MAX_JWK_KEY_OPS {
        return Err(WebCryptoError::Operation);
    }
    for usage in key_ops {
        validate_jwk_member(usage)?;
    }
    Ok(())
}

fn jwk_public_key_use_mask(public_key_use: &str) -> Option<u16> {
    // Match Chromium's composite JWK `use` masks in components/webcrypto/jwk.cc.
    Some(match public_key_use {
        "enc" => {
            jwk_key_op_usage_bit("encrypt")?
                | jwk_key_op_usage_bit("decrypt")?
                | jwk_key_op_usage_bit("deriveKey")?
                | jwk_key_op_usage_bit("deriveBits")?
                | jwk_key_op_usage_bit("wrapKey")?
                | jwk_key_op_usage_bit("unwrapKey")?
        }
        "sig" => jwk_key_op_usage_bit("sign")? | jwk_key_op_usage_bit("verify")?,
        _ => return None,
    })
}
