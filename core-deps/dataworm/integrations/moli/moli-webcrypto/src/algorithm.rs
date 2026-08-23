#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum WebCryptoKeyAlgorithm {
    #[strum(serialize = "aes-cbc")]
    AesCbc,
    #[strum(serialize = "aes-ctr")]
    AesCtr,
    #[strum(serialize = "aes-gcm")]
    AesGcm,
    #[strum(serialize = "aes-kw")]
    AesKw,
    #[strum(serialize = "chacha20-poly1305")]
    Chacha20Poly1305,
    Hkdf,
    Hmac,
    Pbkdf2,
    #[strum(serialize = "rsa-oaep")]
    RsaOaep,
    #[strum(serialize = "rsa-pss")]
    RsaPss,
    #[strum(serialize = "rsassa-pkcs1-v1_5")]
    RsassaPkcs1V15,
    Ecdh,
    Ecdsa,
    Ed25519,
    Ed448,
    X25519,
    X448,
}
impl WebCryptoKeyAlgorithm {
    pub fn is_aes(self) -> bool {
        matches!(
            self,
            Self::AesCbc | Self::AesCtr | Self::AesGcm | Self::AesKw
        )
    }

    pub fn jwk_aes_alg(self, length_bits: usize) -> Option<String> {
        let suffix = match self {
            Self::AesCbc => "CBC",
            Self::AesCtr => "CTR",
            Self::AesGcm => "GCM",
            Self::AesKw => "KW",
            _ => return None,
        };
        if matches!(length_bits, 128 | 192 | 256) {
            Some(format!("A{length_bits}{suffix}"))
        } else {
            None
        }
    }
}
