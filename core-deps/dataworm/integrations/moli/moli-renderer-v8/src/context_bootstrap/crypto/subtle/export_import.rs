use super::*;

pub(crate) enum DerivedKeyTarget {
    Aes {
        algorithm: WebCryptoKeyAlgorithm,
        length_bits: usize,
    },
    Hmac {
        hash: WebCryptoHashAlgorithm,
        length_bits: usize,
    },
    Kdf {
        algorithm: WebCryptoKeyAlgorithm,
    },
}

impl DerivedKeyTarget {
    fn length_bits(&self) -> Option<usize> {
        match self {
            Self::Aes { length_bits, .. } | Self::Hmac { length_bits, .. } => Some(*length_bits),
            Self::Kdf { .. } => None,
        }
    }
}

pub(crate) enum DeriveKeySource {
    X25519,
    X448,
    Ecdh,
    Kdf {
        algorithm: WebCryptoKeyAlgorithm,
        params: KdfDeriveParams,
    },
}

pub(crate) enum DeriveKeySourceMaterial {
    X25519 {
        private_bytes: [u8; 32],
        public_bytes: [u8; 32],
    },
    X448 {
        private_bytes: Vec<u8>,
        public_bytes: Vec<u8>,
    },
    Ecdh {
        private_bytes: Vec<u8>,
        public_bytes: Vec<u8>,
        curve: WebCryptoEcNamedCurve,
    },
    Kdf {
        algorithm: WebCryptoKeyAlgorithm,
        params: KdfDeriveParams,
        base_key: Vec<u8>,
    },
}

pub(crate) fn derived_key_target<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm_object: v8::Local<'s, v8::Object>,
    algorithm: WebCryptoKeyAlgorithm,
    extractable: bool,
    usages: &[String],
) -> Result<DerivedKeyTarget, WebCryptoRejection> {
    match algorithm {
        WebCryptoKeyAlgorithm::AesCbc
        | WebCryptoKeyAlgorithm::AesCtr
        | WebCryptoKeyAlgorithm::AesGcm
        | WebCryptoKeyAlgorithm::AesKw => {
            let length_bits = aes_key_length_bits(scope, algorithm_object, "AesDerivedKeyParams")?;
            if !key_usages_are_valid(algorithm, "secret", usages) {
                return Err(WebCryptoRejection::Syntax);
            }
            Ok(DerivedKeyTarget::Aes {
                algorithm,
                length_bits,
            })
        }
        WebCryptoKeyAlgorithm::Hmac => {
            let hash = required_hmac_hash_algorithm(scope, algorithm_object)?;
            let length_bits = hmac_derived_key_length_bits(scope, algorithm_object, hash)?;
            if !key_usages_are_valid(WebCryptoKeyAlgorithm::Hmac, "secret", usages) {
                return Err(WebCryptoRejection::Syntax);
            }
            Ok(DerivedKeyTarget::Hmac { hash, length_bits })
        }
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => {
            if extractable {
                return Err(WebCryptoRejection::Syntax);
            }
            if !key_usages_are_valid(algorithm, "secret", usages) {
                return Err(WebCryptoRejection::Syntax);
            }
            Ok(DerivedKeyTarget::Kdf { algorithm })
        }
        WebCryptoKeyAlgorithm::Chacha20Poly1305 => Err(WebCryptoRejection::NotSupported),
        _ => Err(WebCryptoRejection::NotSupported),
    }
}

pub(crate) fn resolve_derived_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    promise: &PendingCryptoPromise<'s>,
    target: DerivedKeyTarget,
    extractable: bool,
    usages: &[String],
    key_bytes: Vec<u8>,
) {
    let algorithm = match target {
        DerivedKeyTarget::Aes {
            algorithm,
            length_bits,
        } => build_symmetric_algorithm_object(
            scope,
            webcrypto_algorithm_display_name(algorithm),
            length_bits,
        ),
        DerivedKeyTarget::Hmac { hash, length_bits } => {
            build_hmac_algorithm_object(scope, hash.as_ref(), length_bits)
        }
        DerivedKeyTarget::Kdf { algorithm } => {
            build_named_algorithm_object(scope, webcrypto_algorithm_display_name(algorithm))
        }
    };
    let Some(key) =
        new_crypto_key_object(scope, "secret", algorithm, extractable, usages, &key_bytes)
    else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    promise.resolve(scope, key.into());
}

pub(crate) fn derived_key_clone_payload(
    target: DerivedKeyTarget,
    extractable: bool,
    usages: Vec<String>,
    key_bytes: Vec<u8>,
) -> CryptoKeyClonePayload {
    let algorithm = match target {
        DerivedKeyTarget::Aes {
            algorithm,
            length_bits,
        } => CryptoKeyAlgorithmClonePayload {
            name: webcrypto_algorithm_display_name(algorithm).to_owned(),
            hash_name: None,
            length_bits: Some(length_bits),
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
        DerivedKeyTarget::Hmac { hash, length_bits } => CryptoKeyAlgorithmClonePayload {
            name: "HMAC".to_owned(),
            hash_name: Some(hash.as_ref().to_ascii_uppercase()),
            length_bits: Some(length_bits),
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
        DerivedKeyTarget::Kdf { algorithm } => CryptoKeyAlgorithmClonePayload {
            name: webcrypto_algorithm_display_name(algorithm).to_owned(),
            hash_name: None,
            length_bits: None,
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
    };
    CryptoKeyClonePayload {
        key_type: "secret".to_owned(),
        algorithm,
        extractable,
        usages,
        key_bytes,
    }
}

pub(crate) fn derive_source_material_bytes(
    source: DeriveKeySourceMaterial,
    target_length_bits: Option<usize>,
) -> Result<Vec<u8>, WebCryptoError> {
    match source {
        DeriveKeySourceMaterial::X25519 {
            private_bytes,
            public_bytes,
        } => {
            let length_bits = target_length_bits.unwrap_or(256);
            if length_bits > 256 {
                return Err(WebCryptoError::Operation);
            }
            derive_x25519_bits(&private_bytes, public_bytes, length_bits)
        }
        DeriveKeySourceMaterial::X448 {
            private_bytes,
            public_bytes,
        } => {
            let length_bits = target_length_bits.unwrap_or(448);
            if length_bits > 448 {
                return Err(WebCryptoError::Operation);
            }
            derive_x448_bits(&private_bytes, &public_bytes, length_bits)
        }
        DeriveKeySourceMaterial::Ecdh {
            private_bytes,
            public_bytes,
            curve,
        } => {
            let max_bits = curve.coordinate_len_bytes() * 8;
            let length_bits = target_length_bits.unwrap_or(max_bits);
            if length_bits > max_bits {
                return Err(WebCryptoError::Operation);
            }
            derive_ecdh_bits(&private_bytes, &public_bytes, curve, length_bits)
        }
        DeriveKeySourceMaterial::Kdf {
            algorithm,
            params,
            base_key,
        } => {
            let Some(length_bits) = target_length_bits else {
                return Err(WebCryptoError::Operation);
            };
            derive_kdf_bytes(algorithm, params, &base_key, length_bits)
        }
    }
}

pub(crate) fn crypto_subtle_derive_key_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, &args, &mut rv) {
        return;
    }
    // baseKey is a WebIDL CryptoKey argument, so Chromium rejects a bad key
    // before any source or target algorithm normalization can run.
    let Ok(base_key) = subtle_crypto_key_arg(scope, &args, 1) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(extractable) =
        subtle_required_arg::<webidl::Boolean>(scope, &args, 3, "SubtleCrypto.deriveKey")
    else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(usages) = subtle_key_usages_arg(scope, &args, 4, "SubtleCrypto.deriveKey") else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Some(promise) = PendingCryptoPromise::new(scope, &mut rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let extractable = extractable.0;
    let Some(source_name) = crypto_algorithm_name(scope, args.get(0)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let Ok(source_algorithm) = source_name.parse::<WebCryptoKeyAlgorithm>() else {
        promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
        return;
    };
    let source = match source_algorithm {
        WebCryptoKeyAlgorithm::X25519 => DeriveKeySource::X25519,
        WebCryptoKeyAlgorithm::X448 => DeriveKeySource::X448,
        WebCryptoKeyAlgorithm::Ecdh => DeriveKeySource::Ecdh,
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => {
            let Some(algorithm_object) = args.get(0).to_object(scope) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            let params = match kdf_derive_params(scope, algorithm_object, source_algorithm) {
                Ok(params) => params,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            DeriveKeySource::Kdf {
                algorithm: source_algorithm,
                params,
            }
        }
        _ => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
    };
    let Some(derived_algorithm_object) = args.get(2).to_object(scope) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let Some(derived_name) = crypto_algorithm_name(scope, args.get(2)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let Ok(derived_algorithm) = derived_name.parse::<WebCryptoKeyAlgorithm>() else {
        promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
        return;
    };
    // For ECDH/X* sources, Blink validates the source public/private key pair
    // before derived-key usage compatibility. This preserves the observable
    // InvalidAccessError when `algorithm.public` is a secret/private key, even
    // if the requested target usages would later be a SyntaxError.
    let source = match source {
        DeriveKeySource::X25519 => {
            let (private_bytes, public_bytes) =
                match x25519_derive_material(scope, args.get(0), base_key, "deriveKey") {
                    Ok(material) => material,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                };
            DeriveKeySourceMaterial::X25519 {
                private_bytes,
                public_bytes,
            }
        }
        DeriveKeySource::X448 => {
            let (private_bytes, public_bytes) =
                match x448_derive_material(scope, args.get(0), base_key, "deriveKey") {
                    Ok(material) => material,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                };
            DeriveKeySourceMaterial::X448 {
                private_bytes,
                public_bytes,
            }
        }
        DeriveKeySource::Ecdh => {
            let (private_bytes, public_bytes, curve) =
                match ecdh_derive_material(scope, args.get(0), base_key, "deriveKey") {
                    Ok(material) => material,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                };
            DeriveKeySourceMaterial::Ecdh {
                private_bytes,
                public_bytes,
                curve,
            }
        }
        DeriveKeySource::Kdf { algorithm, params } => {
            // Blink validates the KDF base key before target-key constraints.
            // This is observable when an invalid source key is paired with an
            // unsupported target length: the source-side InvalidAccessError
            // must not be hidden by the target OperationError.
            let base_key = match kdf_base_key_bytes(scope, base_key, algorithm, "deriveKey") {
                Ok(bytes) => bytes,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            DeriveKeySourceMaterial::Kdf {
                algorithm,
                params,
                base_key,
            }
        }
    };
    let target = match derived_key_target(
        scope,
        derived_algorithm_object,
        derived_algorithm,
        extractable,
        &usages,
    ) {
        Ok(target) => target,
        Err(rejection) => {
            promise.reject_webcrypto(scope, rejection);
            return;
        }
    };
    let target_length_bits = target.length_bits();

    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
        let usages = usages.clone();
        spawn_webcrypto_key_task(handle, completion_tx, move || {
            derive_source_material_bytes(source, target_length_bits)
                .map_err(WebCryptoRejection::from)
                .map(|key_bytes| derived_key_clone_payload(target, extractable, usages, key_bytes))
        });
        return;
    }
    let key_bytes = match derive_source_material_bytes(source, target_length_bits) {
        Ok(bytes) => bytes,
        Err(error) => {
            promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
            return;
        }
    };
    resolve_derived_key(scope, &promise, target, extractable, &usages, key_bytes);
}

pub(crate) fn crypto_subtle_export_key_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, &args, &mut rv) {
        return;
    }
    let Ok(format) =
        subtle_required_arg::<webidl::DomString>(scope, &args, 0, "SubtleCrypto.exportKey")
    else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Some(format) = SubtleKeyFormat::parse(&format.0) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(key) = subtle_crypto_key_arg(scope, &args, 1) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Some(key_type) = crypto_key_kind(scope, key) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    if !crypto_key_extractable(scope, key).unwrap_or(false) {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::InvalidAccess);
        return;
    }
    let Some(algorithm) = crypto_key_algorithm_name(scope, key)
        .and_then(|name| name.parse::<WebCryptoKeyAlgorithm>().ok())
    else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::NotSupported);
        return;
    };
    if let Some(rejection) = key_export_format_error(format, algorithm, &key_type) {
        set_rejected_webcrypto_promise(scope, &mut rv, rejection);
        return;
    }

    let snapshot =
        match export_key_snapshot_from_crypto_key(scope, key, format, algorithm, key_type) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                set_rejected_webcrypto_promise(scope, &mut rv, error);
                return;
            }
        };
    let Some(promise) = PendingCryptoPromise::new(scope, &mut rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
        spawn_webcrypto_result_task(handle, completion_tx, move || {
            export_key_task_result(snapshot)
        });
        return;
    }

    resolve_webcrypto_task_result(scope, &promise, export_key_task_result(snapshot));
}

pub(crate) struct ExportKeySnapshot {
    format: SubtleKeyFormat,
    algorithm: WebCryptoKeyAlgorithm,
    key_type: String,
    key_bytes: Vec<u8>,
    usages: Vec<String>,
    hmac_hash: Option<WebCryptoHashAlgorithm>,
    rsa_hash: Option<WebCryptoHashAlgorithm>,
}

pub(crate) fn export_key_snapshot_from_crypto_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key: v8::Local<'s, v8::Object>,
    format: SubtleKeyFormat,
    algorithm: WebCryptoKeyAlgorithm,
    key_type: String,
) -> Result<ExportKeySnapshot, WebCryptoRejection> {
    let key_bytes = crypto_key_bytes(scope, key).ok_or(WebCryptoRejection::Type)?;
    let usages = if format.is_jwk() {
        crypto_key_usages(scope, key).unwrap_or_default()
    } else {
        Vec::new()
    };
    let hmac_hash = if format.is_jwk() && algorithm == WebCryptoKeyAlgorithm::Hmac {
        Some(crypto_key_hmac_hash_algorithm(scope, key).ok_or(WebCryptoRejection::NotSupported)?)
    } else {
        None
    };
    let rsa_hash = if format.is_jwk()
        && matches!(
            algorithm,
            WebCryptoKeyAlgorithm::RsaOaep
                | WebCryptoKeyAlgorithm::RsaPss
                | WebCryptoKeyAlgorithm::RsassaPkcs1V15
        ) {
        Some(crypto_key_rsa_hash_algorithm(scope, key).ok_or(WebCryptoRejection::Type)?)
    } else {
        None
    };

    Ok(ExportKeySnapshot {
        format,
        algorithm,
        key_type,
        key_bytes,
        usages,
        hmac_hash,
        rsa_hash,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubtleKeyFormat {
    Raw,
    Jwk,
    Spki,
    Pkcs8,
    RawPublic,
    RawPrivate,
    RawSeed,
    RawSecret,
}

impl SubtleKeyFormat {
    pub(crate) fn parse(format: &str) -> Option<Self> {
        Some(match format {
            "raw" => Self::Raw,
            "jwk" => Self::Jwk,
            "spki" => Self::Spki,
            "pkcs8" => Self::Pkcs8,
            "raw-public" => Self::RawPublic,
            "raw-private" => Self::RawPrivate,
            "raw-seed" => Self::RawSeed,
            "raw-secret" => Self::RawSecret,
            _ => return None,
        })
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Jwk => "jwk",
            Self::Spki => "spki",
            Self::Pkcs8 => "pkcs8",
            Self::RawPublic => "raw-public",
            Self::RawPrivate => "raw-private",
            Self::RawSeed => "raw-seed",
            Self::RawSecret => "raw-secret",
        }
    }

    fn is_jwk(self) -> bool {
        matches!(self, Self::Jwk)
    }
}

pub(crate) enum ExportedKeyMaterial {
    Bytes(Vec<u8>),
    JsonWebKey {
        bytes: Vec<u8>,
        value: serde_json::Value,
    },
}

pub(crate) fn json_web_key_material<T: serde::Serialize>(
    value: T,
) -> Result<ExportedKeyMaterial, WebCryptoRejection> {
    // `wrapKey("jwk", ...)` encrypts the serialized JSON bytes, while public
    // `exportKey("jwk", ...)` resolves with an object. Produce both from the
    // same typed JWK value so deterministic wrappers such as AES-KW do not
    // drift from the public export representation.
    let bytes = serde_json::to_vec(&value).map_err(|_| WebCryptoRejection::Type)?;
    let value = serde_json::to_value(value).map_err(|_| WebCryptoRejection::Type)?;
    Ok(ExportedKeyMaterial::JsonWebKey { bytes, value })
}

pub(crate) fn x25519_snapshot_bytes(bytes: &[u8]) -> Result<[u8; 32], WebCryptoRejection> {
    bytes.try_into().map_err(|_| WebCryptoRejection::Type)
}

pub(crate) fn export_key_material(
    snapshot: ExportKeySnapshot,
) -> Result<ExportedKeyMaterial, WebCryptoRejection> {
    let ExportKeySnapshot {
        format,
        algorithm,
        key_type,
        key_bytes,
        usages,
        hmac_hash,
        rsa_hash,
    } = snapshot;
    match (format.as_str(), algorithm, key_type.as_str()) {
        (
            "raw" | "raw-secret",
            WebCryptoKeyAlgorithm::AesCbc
            | WebCryptoKeyAlgorithm::AesCtr
            | WebCryptoKeyAlgorithm::AesGcm
            | WebCryptoKeyAlgorithm::AesKw
            | WebCryptoKeyAlgorithm::Hmac,
            "secret",
        )
        | ("raw-secret", WebCryptoKeyAlgorithm::Chacha20Poly1305, "secret")
        | ("raw" | "raw-public", WebCryptoKeyAlgorithm::X25519, "public") => {
            Ok(ExportedKeyMaterial::Bytes(key_bytes))
        }
        (
            "spki",
            WebCryptoKeyAlgorithm::RsaOaep
            | WebCryptoKeyAlgorithm::RsaPss
            | WebCryptoKeyAlgorithm::RsassaPkcs1V15
            | WebCryptoKeyAlgorithm::Ecdh
            | WebCryptoKeyAlgorithm::Ecdsa,
            "public",
        )
        | (
            "pkcs8",
            WebCryptoKeyAlgorithm::RsaOaep
            | WebCryptoKeyAlgorithm::RsaPss
            | WebCryptoKeyAlgorithm::RsassaPkcs1V15
            | WebCryptoKeyAlgorithm::Ecdh
            | WebCryptoKeyAlgorithm::Ecdsa,
            "private",
        ) => Ok(ExportedKeyMaterial::Bytes(key_bytes)),
        (
            "raw" | "raw-public",
            WebCryptoKeyAlgorithm::Ecdh | WebCryptoKeyAlgorithm::Ecdsa,
            "public",
        ) => export_ec_raw_public_key(&key_bytes)
            .map(ExportedKeyMaterial::Bytes)
            .map_err(WebCryptoRejection::from),
        (
            "raw" | "raw-public",
            WebCryptoKeyAlgorithm::Ed25519
            | WebCryptoKeyAlgorithm::Ed448
            | WebCryptoKeyAlgorithm::X448,
            "public",
        ) => Ok(ExportedKeyMaterial::Bytes(key_bytes)),
        (
            "spki",
            WebCryptoKeyAlgorithm::Ed25519
            | WebCryptoKeyAlgorithm::Ed448
            | WebCryptoKeyAlgorithm::X448,
            "public",
        ) => {
            let curve =
                okp_curve_for_algorithm(algorithm).ok_or(WebCryptoRejection::NotSupported)?;
            export_okp_spki_public_key(curve, &key_bytes)
                .map(ExportedKeyMaterial::Bytes)
                .map_err(WebCryptoRejection::from)
        }
        (
            "pkcs8",
            WebCryptoKeyAlgorithm::Ed25519
            | WebCryptoKeyAlgorithm::Ed448
            | WebCryptoKeyAlgorithm::X448,
            "private",
        ) => {
            let curve =
                okp_curve_for_algorithm(algorithm).ok_or(WebCryptoRejection::NotSupported)?;
            export_okp_pkcs8_private_key(curve, &key_bytes)
                .map(ExportedKeyMaterial::Bytes)
                .map_err(WebCryptoRejection::from)
        }
        (
            "jwk",
            WebCryptoKeyAlgorithm::AesCbc
            | WebCryptoKeyAlgorithm::AesCtr
            | WebCryptoKeyAlgorithm::AesGcm
            | WebCryptoKeyAlgorithm::AesKw,
            "secret",
        ) => json_web_key_material(
            export_aes_jwk(algorithm, &key_bytes, usages, true)
                .map_err(WebCryptoRejection::from)?,
        ),
        ("jwk", WebCryptoKeyAlgorithm::Chacha20Poly1305, "secret") => json_web_key_material(
            export_chacha20_poly1305_jwk(&key_bytes, usages, true)
                .map_err(WebCryptoRejection::from)?,
        ),
        ("jwk", WebCryptoKeyAlgorithm::Hmac, "secret") => json_web_key_material(export_hmac_jwk(
            hmac_hash.ok_or(WebCryptoRejection::NotSupported)?,
            &key_bytes,
            usages,
            true,
        )),
        (
            "jwk",
            WebCryptoKeyAlgorithm::RsaOaep
            | WebCryptoKeyAlgorithm::RsaPss
            | WebCryptoKeyAlgorithm::RsassaPkcs1V15,
            "public",
        ) => json_web_key_material(
            export_rsa_jwk_public_key(
                &key_bytes,
                algorithm,
                rsa_hash.ok_or(WebCryptoRejection::Type)?,
                usages,
                true,
            )
            .map_err(WebCryptoRejection::from)?,
        ),
        (
            "jwk",
            WebCryptoKeyAlgorithm::RsaOaep
            | WebCryptoKeyAlgorithm::RsaPss
            | WebCryptoKeyAlgorithm::RsassaPkcs1V15,
            "private",
        ) => json_web_key_material(
            export_rsa_jwk_private_key(
                &key_bytes,
                algorithm,
                rsa_hash.ok_or(WebCryptoRejection::Type)?,
                usages,
                true,
            )
            .map_err(WebCryptoRejection::from)?,
        ),
        ("jwk", WebCryptoKeyAlgorithm::Ecdh | WebCryptoKeyAlgorithm::Ecdsa, "public") => {
            json_web_key_material(
                export_ec_jwk_public_key(&key_bytes, algorithm, usages, true)
                    .map_err(WebCryptoRejection::from)?,
            )
        }
        ("jwk", WebCryptoKeyAlgorithm::Ecdh | WebCryptoKeyAlgorithm::Ecdsa, "private") => {
            json_web_key_material(
                export_ec_jwk_private_key(&key_bytes, algorithm, usages, true)
                    .map_err(WebCryptoRejection::from)?,
            )
        }
        (
            "jwk",
            WebCryptoKeyAlgorithm::Ed25519
            | WebCryptoKeyAlgorithm::Ed448
            | WebCryptoKeyAlgorithm::X448,
            "public",
        ) => {
            let curve =
                okp_curve_for_algorithm(algorithm).ok_or(WebCryptoRejection::NotSupported)?;
            json_web_key_material(export_okp_jwk_public_key(curve, &key_bytes, usages, true))
        }
        (
            "jwk",
            WebCryptoKeyAlgorithm::Ed25519
            | WebCryptoKeyAlgorithm::Ed448
            | WebCryptoKeyAlgorithm::X448,
            "private",
        ) => {
            let curve =
                okp_curve_for_algorithm(algorithm).ok_or(WebCryptoRejection::NotSupported)?;
            json_web_key_material(
                export_okp_jwk_private_key(curve, &key_bytes, usages, true)
                    .map_err(WebCryptoRejection::from)?,
            )
        }
        ("spki", WebCryptoKeyAlgorithm::X25519, "public") => {
            let public_key = x25519_snapshot_bytes(&key_bytes)?;
            Ok(ExportedKeyMaterial::Bytes(export_x25519_spki_public_key(
                &public_key,
            )))
        }
        ("pkcs8", WebCryptoKeyAlgorithm::X25519, "private") => {
            let private_key = x25519_snapshot_bytes(&key_bytes)?;
            Ok(ExportedKeyMaterial::Bytes(export_x25519_pkcs8_private_key(
                &private_key,
            )))
        }
        ("jwk", WebCryptoKeyAlgorithm::X25519, "public") => {
            let public_key = x25519_snapshot_bytes(&key_bytes)?;
            json_web_key_material(export_x25519_jwk_public_key(&public_key, usages, true))
        }
        ("jwk", WebCryptoKeyAlgorithm::X25519, "private") => {
            let private_key = x25519_snapshot_bytes(&key_bytes)?;
            json_web_key_material(
                export_x25519_jwk_private_key(&private_key, usages, true)
                    .map_err(WebCryptoRejection::from)?,
            )
        }
        ("raw" | "raw-public", WebCryptoKeyAlgorithm::X25519, "private")
        | ("spki", WebCryptoKeyAlgorithm::X25519, "private")
        | ("pkcs8", WebCryptoKeyAlgorithm::X25519, "public") => {
            Err(WebCryptoRejection::InvalidAccess)
        }
        _ => Err(key_export_format_error(format, algorithm, &key_type)
            .unwrap_or(WebCryptoRejection::NotSupported)),
    }
}

pub(crate) fn export_key_task_result(
    snapshot: ExportKeySnapshot,
) -> Result<WebCryptoTaskResult, WebCryptoRejection> {
    match export_key_material(snapshot)? {
        ExportedKeyMaterial::Bytes(bytes) => Ok(WebCryptoTaskResult::Bytes(bytes)),
        ExportedKeyMaterial::JsonWebKey { value, .. } => Ok(WebCryptoTaskResult::JsonWebKey(value)),
    }
}

pub(crate) fn export_key_material_from_snapshot(
    snapshot: ExportKeySnapshot,
) -> Result<Vec<u8>, WebCryptoRejection> {
    match export_key_material(snapshot)? {
        ExportedKeyMaterial::Bytes(bytes) => Ok(bytes),
        ExportedKeyMaterial::JsonWebKey { bytes, .. } => Ok(bytes),
    }
}

pub(crate) fn wrap_key_task_result<F>(
    snapshot: ExportKeySnapshot,
    operation: F,
) -> Result<WebCryptoTaskResult, WebCryptoRejection>
where
    F: FnOnce(Vec<u8>) -> Result<Vec<u8>, WebCryptoError>,
{
    let key_material = export_key_material_from_snapshot(snapshot)?;
    operation(key_material)
        .map(WebCryptoTaskResult::Bytes)
        .map_err(WebCryptoRejection::from)
}

pub(crate) fn resolve_webcrypto_task_result<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    promise: &PendingCryptoPromise<'s>,
    result: Result<WebCryptoTaskResult, WebCryptoRejection>,
) {
    match result {
        Ok(WebCryptoTaskResult::Bytes(bytes)) => {
            match blob::array_buffer_from_bytes(scope, bytes) {
                Some(buffer) => promise.resolve(scope, buffer.into()),
                None => promise.reject_webcrypto(scope, WebCryptoRejection::Type),
            }
        }
        Ok(WebCryptoTaskResult::JsonWebKey(value)) => match serde_v8::to_v8(scope, value) {
            Ok(value) => promise.resolve(scope, value),
            Err(_) => promise.reject_webcrypto(scope, WebCryptoRejection::Type),
        },
        Ok(_) => promise.reject_webcrypto(scope, WebCryptoRejection::Type),
        Err(error) => promise.reject_webcrypto(scope, error),
    }
}

pub(crate) fn crypto_subtle_import_key_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, &args, &mut rv) {
        return;
    }
    let Ok(format) =
        subtle_required_arg::<webidl::DomString>(scope, &args, 0, "SubtleCrypto.importKey")
    else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let format = format.0;
    if !subtle_key_format_is_valid(&format) {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    }
    let Ok(extractable) =
        subtle_required_arg::<webidl::Boolean>(scope, &args, 3, "SubtleCrypto.importKey")
    else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(usages) = subtle_key_usages_arg(scope, &args, 4, "SubtleCrypto.importKey") else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Some(promise) = PendingCryptoPromise::new(scope, &mut rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let key_data = match format.as_str() {
        // After KeyFormat and KeyUsage validation, WebCrypto copies
        // BufferSource key data before normalizing the algorithm. Chromium has
        // regression coverage where an algorithm.name getter mutates the
        // original typed array; imported raw bytes must still reflect the
        // pre-normalization value. Chromium treats the modern raw-* KeyFormat
        // extensions as byte-form inputs at the same boundary.
        "raw" | "raw-public" | "raw-private" | "raw-seed" | "raw-secret" => {
            match subtle_buffer_source_value_with_max_rejection(
                scope,
                args.get(1),
                webidl::Context::argument("SubtleCrypto.importKey", 2),
                MAX_RAW_KEY_IMPORT_BYTES,
                false,
            ) {
                Ok(bytes) => ImportKeyData::Bytes(bytes),
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            }
        }
        "spki" | "pkcs8" => match subtle_buffer_source_value_with_max_rejection(
            scope,
            args.get(1),
            webidl::Context::argument("SubtleCrypto.importKey", 2),
            MAX_DER_KEY_BYTES,
            false,
        ) {
            Ok(bytes) => ImportKeyData::Bytes(bytes),
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        },
        "jwk" => match parse_json_web_key(scope, args.get(1)) {
            Ok(jwk) => ImportKeyData::Jwk(Box::new(jwk)),
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        },
        _ => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
    };
    let Some(name) = crypto_algorithm_name(scope, args.get(2)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let Ok(algorithm_name) = name.parse::<WebCryptoKeyAlgorithm>() else {
        promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
        return;
    };

    match algorithm_name {
        WebCryptoKeyAlgorithm::AesCbc
        | WebCryptoKeyAlgorithm::AesCtr
        | WebCryptoKeyAlgorithm::AesGcm
        | WebCryptoKeyAlgorithm::AesKw => import_aes_key(
            scope,
            key_data,
            &format,
            algorithm_name,
            extractable.0,
            &usages,
            &promise,
        ),
        WebCryptoKeyAlgorithm::Chacha20Poly1305 => {
            import_chacha20_poly1305_key(scope, key_data, &format, extractable.0, &usages, &promise)
        }
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => import_kdf_key(
            scope,
            key_data,
            &format,
            algorithm_name,
            extractable.0,
            &usages,
            &promise,
        ),
        WebCryptoKeyAlgorithm::Hmac => import_hmac_key(
            scope,
            args.get(2),
            key_data,
            &format,
            extractable.0,
            &usages,
            &promise,
        ),
        WebCryptoKeyAlgorithm::X25519 => {
            import_x25519_key(scope, key_data, &format, extractable.0, &usages, &promise)
        }
        WebCryptoKeyAlgorithm::RsaOaep
        | WebCryptoKeyAlgorithm::RsaPss
        | WebCryptoKeyAlgorithm::RsassaPkcs1V15 => {
            let hash = match rsa_import_hash_rejection(scope, args.get(2)) {
                Ok(hash) => hash,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            import_rsa_key(
                scope,
                key_data,
                &format,
                algorithm_name,
                hash,
                extractable.0,
                &usages,
                &promise,
            )
        }
        WebCryptoKeyAlgorithm::Ecdh | WebCryptoKeyAlgorithm::Ecdsa => {
            let Some(algorithm_object) = args.get(2).to_object(scope) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            let curve = match ec_named_curve_param(scope, algorithm_object) {
                Ok(curve) => curve,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            import_ec_key(
                scope,
                key_data,
                &format,
                algorithm_name,
                curve,
                extractable.0,
                &usages,
                &promise,
            )
        }
        WebCryptoKeyAlgorithm::Ed25519
        | WebCryptoKeyAlgorithm::Ed448
        | WebCryptoKeyAlgorithm::X448 => {
            let curve = match required_okp_curve_for_algorithm_rejection(algorithm_name) {
                Ok(curve) => curve,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            import_okp_key(
                scope,
                key_data,
                &format,
                algorithm_name,
                curve,
                extractable.0,
                &usages,
                &promise,
            )
        }
    }
}

pub(crate) fn subtle_key_format_is_valid(format: &str) -> bool {
    // WebCrypto exposes KeyFormat as an enum at the WebIDL boundary; invalid
    // strings are TypeError cases, not unsupported algorithm/format cases.
    // Chromium also accepts the modern raw-* extensions here even when a
    // specific algorithm later rejects one with NotSupportedError.
    SubtleKeyFormat::parse(format).is_some()
}

pub(crate) enum ImportKeyData {
    Bytes(Vec<u8>),
    Jwk(Box<ParsedJsonWebKey>),
}

pub(crate) fn import_aes_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    // Chromium's per-algorithm ImportKey entrypoint rejects unsupported key
    // formats before running key-creation usage checks.
    let key_bytes = match format {
        "raw" | "raw-secret" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if key_usages_contain_invalid(algorithm_name, "secret", usages) {
                    promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                    return;
                }
                bytes
            }
            ImportKeyData::Jwk(_) => {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            }
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => {
                if key_usages_contain_invalid(algorithm_name, "secret", usages) {
                    promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                    return;
                }
                match import_aes_jwk(*value, algorithm_name, extractable, usages) {
                    Ok(bytes) => bytes,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                }
            }
            ImportKeyData::Bytes(_) => {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            }
        },
        "spki" | "pkcs8" => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
        _ => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
    };

    // Chromium rejects non-empty invalid AES usages before key material, but
    // reports backend key-length failures before the final empty-usages check.
    let length_bits = match validate_aes_key_bytes(&key_bytes) {
        Ok(length_bits) => length_bits,
        Err(error) => {
            promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
            return;
        }
    };
    if usages.is_empty() {
        promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
        return;
    }
    let algorithm = build_symmetric_algorithm_object(
        scope,
        webcrypto_algorithm_display_name(algorithm_name),
        length_bits,
    );
    let Some(key) =
        new_crypto_key_object(scope, "secret", algorithm, extractable, usages, &key_bytes)
    else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    promise.resolve(scope, key.into());
}

pub(crate) fn import_kdf_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    // KDF keys only support raw import. Unsupported formats should therefore
    // surface as NotSupportedError before KDF-specific creation checks such as
    // usage validation or the non-extractable requirement.
    let key_bytes = match format {
        "raw" | "raw-secret" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "secret", usages) {
                    promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                    return;
                }
                if extractable {
                    promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                    return;
                }
                bytes
            }
            ImportKeyData::Jwk(_) => {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            }
        },
        "jwk" | "spki" | "pkcs8" => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
        _ => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
    };
    let algorithm =
        build_named_algorithm_object(scope, webcrypto_algorithm_display_name(algorithm_name));
    let Some(key) = new_crypto_key_object(scope, "secret", algorithm, false, usages, &key_bytes)
    else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    promise.resolve(scope, key.into());
}

pub(crate) fn import_hmac_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm: v8::Local<'s, v8::Value>,
    key_data: ImportKeyData,
    format: &str,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    let params = match hmac_import_params_rejection(scope, algorithm) {
        Ok(params) => params,
        Err(rejection) => {
            promise.reject_webcrypto(scope, rejection);
            return;
        }
    };
    import_hmac_key_with_params(
        scope,
        params,
        key_data,
        format,
        extractable,
        usages,
        promise,
    );
}

pub(crate) fn import_hmac_key_with_params<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    params: HmacImportParams,
    key_data: ImportKeyData,
    format: &str,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    // HMAC algorithm parameters are normalized before the backend ImportKey
    // switch, but unsupported key formats still reject before usage checks.
    let key_bytes = match format {
        "raw" | "raw-secret" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(WebCryptoKeyAlgorithm::Hmac, "secret", usages) {
                    promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                    return;
                }
                bytes
            }
            ImportKeyData::Jwk(_) => {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            }
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => {
                if !key_usages_are_valid(WebCryptoKeyAlgorithm::Hmac, "secret", usages) {
                    promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                    return;
                }
                match import_hmac_jwk(*value, params.hash, extractable, usages) {
                    Ok(bytes) => bytes,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                }
            }
            ImportKeyData::Bytes(_) => {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            }
        },
        "spki" | "pkcs8" => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
        _ => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
    };
    let (key_bytes, length_bits) =
        match validate_hmac_import_key_bytes(key_bytes, params.length_bits) {
            Ok(imported) => imported,
            Err(error) => {
                promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                return;
            }
        };

    let algorithm = build_hmac_algorithm_object(scope, params.hash.as_ref(), length_bits);
    let Some(key) =
        new_crypto_key_object(scope, "secret", algorithm, extractable, usages, &key_bytes)
    else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    promise.resolve(scope, key.into());
}

pub(crate) fn import_x25519_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key_data: ImportKeyData,
    format: &str,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, promise) {
        let format = format.to_owned();
        let usages = usages.to_vec();
        spawn_webcrypto_key_task(handle, completion_tx, move || {
            import_x25519_key_payload(key_data, &format, extractable, usages)
        });
        return;
    }

    match import_x25519_key_payload(key_data, format, extractable, usages.to_vec()) {
        Ok(payload) => resolve_crypto_key_payload(scope, promise, payload),
        Err(rejection) => promise.reject_webcrypto(scope, rejection),
    }
}

pub(crate) fn import_rsa_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    hash: WebCryptoHashAlgorithm,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, promise) {
        let format = format.to_owned();
        let usages = usages.to_vec();
        spawn_webcrypto_key_task(handle, completion_tx, move || {
            import_rsa_key_payload(key_data, &format, algorithm_name, hash, extractable, usages)
        });
        return;
    }

    match import_rsa_key_payload(
        key_data,
        format,
        algorithm_name,
        hash,
        extractable,
        usages.to_vec(),
    ) {
        Ok(payload) => resolve_crypto_key_payload(scope, promise, payload),
        Err(rejection) => promise.reject_webcrypto(scope, rejection),
    }
}

pub(crate) fn import_ec_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    curve: WebCryptoEcNamedCurve,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, promise) {
        let format = format.to_owned();
        let usages = usages.to_vec();
        spawn_webcrypto_key_task(handle, completion_tx, move || {
            import_ec_key_payload(
                key_data,
                &format,
                algorithm_name,
                curve,
                extractable,
                usages,
            )
        });
        return;
    }

    match import_ec_key_payload(
        key_data,
        format,
        algorithm_name,
        curve,
        extractable,
        usages.to_vec(),
    ) {
        Ok(payload) => resolve_crypto_key_payload(scope, promise, payload),
        Err(rejection) => promise.reject_webcrypto(scope, rejection),
    }
}

pub(crate) fn import_okp_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    curve: WebCryptoOkpCurve,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, promise) {
        let format = format.to_owned();
        let usages = usages.to_vec();
        spawn_webcrypto_key_task(handle, completion_tx, move || {
            import_okp_key_payload(
                key_data,
                &format,
                algorithm_name,
                curve,
                extractable,
                usages,
            )
        });
        return;
    }

    match import_okp_key_payload(
        key_data,
        format,
        algorithm_name,
        curve,
        extractable,
        usages.to_vec(),
    ) {
        Ok(payload) => resolve_crypto_key_payload(scope, promise, payload),
        Err(rejection) => promise.reject_webcrypto(scope, rejection),
    }
}

pub(crate) fn import_hmac_jwk(
    jwk: ParsedJsonWebKey,
    expected_hash: WebCryptoHashAlgorithm,
    extractable: bool,
    usages: &[String],
) -> Result<Vec<u8>, WebCryptoRejection> {
    import_hmac_jwk_key(&jwk.into_hmac(), expected_hash, extractable, usages)
        .map_err(WebCryptoRejection::from)
}

pub(crate) fn import_aes_jwk(
    jwk: ParsedJsonWebKey,
    expected_algorithm: WebCryptoKeyAlgorithm,
    extractable: bool,
    usages: &[String],
) -> Result<Vec<u8>, WebCryptoRejection> {
    import_aes_jwk_key(&jwk.into_aes(), expected_algorithm, extractable, usages)
        .map_err(WebCryptoRejection::from)
}

pub(crate) fn import_chacha20_poly1305_jwk(
    jwk: ParsedJsonWebKey,
    extractable: bool,
    usages: &[String],
) -> Result<Vec<u8>, WebCryptoRejection> {
    import_chacha20_poly1305_jwk_key(&jwk.into_chacha20_poly1305(), extractable, usages)
        .map_err(WebCryptoRejection::from)
}

pub(crate) fn import_x25519_jwk(
    jwk: ParsedJsonWebKey,
    extractable: bool,
    usages: &[String],
) -> Result<X25519ImportedKey, WebCryptoRejection> {
    import_x25519_jwk_key(&jwk.into_okp(), extractable, usages).map_err(WebCryptoRejection::from)
}

pub(crate) fn import_rsa_jwk(
    jwk: ParsedJsonWebKey,
    algorithm: WebCryptoKeyAlgorithm,
    hash: WebCryptoHashAlgorithm,
    extractable: bool,
    usages: &[String],
) -> Result<RsaImportedKey, WebCryptoRejection> {
    import_rsa_jwk_key(&jwk.into_rsa(), algorithm, hash, extractable, usages)
        .map_err(WebCryptoRejection::from)
}

pub(crate) fn import_ec_jwk(
    jwk: ParsedJsonWebKey,
    algorithm: WebCryptoKeyAlgorithm,
    curve: WebCryptoEcNamedCurve,
    extractable: bool,
    usages: &[String],
) -> Result<EcImportedKey, WebCryptoRejection> {
    import_ec_jwk_key(&jwk.into_ec(), algorithm, curve, extractable, usages)
        .map_err(WebCryptoRejection::from)
}

pub(crate) fn import_okp_jwk(
    jwk: ParsedJsonWebKey,
    curve: WebCryptoOkpCurve,
    extractable: bool,
    usages: &[String],
) -> Result<OkpImportedKey, WebCryptoRejection> {
    import_okp_jwk_key(&jwk.into_okp(), curve, extractable, usages)
        .map_err(WebCryptoRejection::from)
}

#[derive(serde::Deserialize)]
pub(crate) struct ParsedJsonWebKey {
    kty: Option<String>,
    #[serde(rename = "use")]
    public_key_use: Option<String>,
    key_ops: Option<Vec<String>>,
    alg: Option<String>,
    ext: Option<bool>,
    crv: Option<String>,
    x: Option<String>,
    y: Option<String>,
    d: Option<String>,
    n: Option<String>,
    e: Option<String>,
    p: Option<String>,
    q: Option<String>,
    dp: Option<String>,
    dq: Option<String>,
    qi: Option<String>,
    k: Option<String>,
}

impl ParsedJsonWebKey {
    fn into_hmac(self) -> HmacJsonWebKeyImport {
        HmacJsonWebKeyImport {
            kty: self.kty,
            k: self.k,
            alg: self.alg,
            key_ops: self.key_ops,
            ext: self.ext,
            public_key_use: self.public_key_use,
        }
    }

    fn into_aes(self) -> AesJsonWebKeyImport {
        AesJsonWebKeyImport {
            kty: self.kty,
            k: self.k,
            alg: self.alg,
            key_ops: self.key_ops,
            ext: self.ext,
            public_key_use: self.public_key_use,
        }
    }

    fn into_chacha20_poly1305(self) -> Chacha20Poly1305JsonWebKeyImport {
        Chacha20Poly1305JsonWebKeyImport {
            kty: self.kty,
            k: self.k,
            alg: self.alg,
            key_ops: self.key_ops,
            ext: self.ext,
            public_key_use: self.public_key_use,
        }
    }

    fn into_okp(self) -> OkpJsonWebKeyImport {
        OkpJsonWebKeyImport {
            kty: self.kty,
            crv: self.crv,
            x: self.x,
            d: self.d,
            alg: self.alg,
            key_ops: self.key_ops,
            ext: self.ext,
            public_key_use: self.public_key_use,
        }
    }

    fn into_rsa(self) -> RsaJsonWebKeyImport {
        RsaJsonWebKeyImport {
            kty: self.kty,
            n: self.n,
            e: self.e,
            d: self.d,
            p: self.p,
            q: self.q,
            dp: self.dp,
            dq: self.dq,
            qi: self.qi,
            alg: self.alg,
            key_ops: self.key_ops,
            ext: self.ext,
            public_key_use: self.public_key_use,
        }
    }

    fn into_ec(self) -> EcJsonWebKeyImport {
        EcJsonWebKeyImport {
            kty: self.kty,
            crv: self.crv,
            x: self.x,
            y: self.y,
            d: self.d,
            alg: self.alg,
            key_ops: self.key_ops,
            ext: self.ext,
            public_key_use: self.public_key_use,
        }
    }
}

pub(crate) fn parse_json_web_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
) -> Result<ParsedJsonWebKey, WebCryptoRejection> {
    // JsonWebKey is a WebIDL dictionary argument. Blink converts/copies its
    // members before NormalizeAlgorithm, so later algorithm getter side effects
    // cannot change what importKey sees.
    let kty = json_web_key_dom_string_member(scope, value, "kty")?;
    let public_key_use = json_web_key_dom_string_member(scope, value, "use")?;
    let key_ops = json_web_key_key_ops_member(scope, value)?;
    let alg = json_web_key_dom_string_member(scope, value, "alg")?;
    let ext = json_web_key_boolean_member(scope, value, "ext")?;
    let crv = json_web_key_dom_string_member(scope, value, "crv")?;
    let x = json_web_key_dom_string_member(scope, value, "x")?;
    let y = json_web_key_dom_string_member(scope, value, "y")?;
    let d = json_web_key_dom_string_member(scope, value, "d")?;
    let n = json_web_key_dom_string_member(scope, value, "n")?;
    let e = json_web_key_dom_string_member(scope, value, "e")?;
    let p = json_web_key_dom_string_member(scope, value, "p")?;
    let q = json_web_key_dom_string_member(scope, value, "q")?;
    let dp = json_web_key_dom_string_member(scope, value, "dp")?;
    let dq = json_web_key_dom_string_member(scope, value, "dq")?;
    let qi = json_web_key_dom_string_member(scope, value, "qi")?;
    let k = json_web_key_dom_string_member(scope, value, "k")?;
    Ok(ParsedJsonWebKey {
        kty,
        public_key_use,
        key_ops,
        alg,
        ext,
        crv,
        x,
        y,
        d,
        n,
        e,
        p,
        q,
        dp,
        dq,
        qi,
        k,
    })
}

pub(crate) fn json_web_key_object<'s>(
    value: v8::Local<'s, v8::Value>,
) -> Result<Option<v8::Local<'s, v8::Object>>, WebCryptoRejection> {
    if value.is_null_or_undefined() {
        return Ok(None);
    }
    v8::Local::<v8::Object>::try_from(value)
        .map(Some)
        .map_err(|_| WebCryptoRejection::Type)
}

pub(crate) fn json_web_key_dom_string_member<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    member: &'static str,
) -> Result<Option<String>, WebCryptoRejection> {
    let Some(object) = json_web_key_object(value)? else {
        return Ok(None);
    };
    let value = webidl::optional_member::<webidl::DomString>(
        scope,
        object,
        member,
        webidl::Context::member("JsonWebKey", member),
    )
    .map(|value| value.map(Into::into))
    .map_err(|_| WebCryptoRejection::Type)?;
    if value
        .as_ref()
        .is_some_and(|value: &String| value.len() > MAX_JWK_MEMBER_BYTES)
    {
        return Err(WebCryptoRejection::Operation);
    }
    Ok(value)
}

pub(crate) fn json_web_key_boolean_member<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    member: &'static str,
) -> Result<Option<bool>, WebCryptoRejection> {
    let Some(object) = json_web_key_object(value)? else {
        return Ok(None);
    };
    webidl::optional_member::<webidl::Boolean>(
        scope,
        object,
        member,
        webidl::Context::member("JsonWebKey", member),
    )
    .map(|value| value.map(|value| value.0))
    .map_err(|_| WebCryptoRejection::Type)
}

pub(crate) fn json_web_key_key_ops_member<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
) -> Result<Option<Vec<String>>, WebCryptoRejection> {
    let Some(object) = json_web_key_object(value)? else {
        return Ok(None);
    };
    let key_ops = webidl::optional_member::<webidl::Sequence<webidl::DomString>>(
        scope,
        object,
        "key_ops",
        webidl::Context::member("JsonWebKey", "key_ops"),
    )
    .map(|value| value.map(|value| value.0.into_iter().map(|value| value.0).collect::<Vec<_>>()))
    .map_err(|_| WebCryptoRejection::Type)?;
    if let Some(key_ops) = key_ops.as_ref()
        && (key_ops.len() > MAX_JWK_KEY_OPS
            || key_ops
                .iter()
                .any(|usage| usage.len() > MAX_JWK_MEMBER_BYTES))
    {
        return Err(WebCryptoRejection::Operation);
    }
    Ok(key_ops)
}
