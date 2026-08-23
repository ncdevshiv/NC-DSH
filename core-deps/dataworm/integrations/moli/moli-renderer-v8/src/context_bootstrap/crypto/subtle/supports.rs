use super::*;

pub(crate) fn crypto_subtle_generate_key_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, &args, &mut rv) {
        return;
    }
    // Chromium parses generateKey keyUsages before NormalizeAlgorithm, so an
    // invalid usage list must not trigger algorithm.name getter side effects.
    let Ok(extractable) =
        subtle_required_arg::<webidl::Boolean>(scope, &args, 1, "SubtleCrypto.generateKey")
    else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(usages) = subtle_key_usages_arg(scope, &args, 2, "SubtleCrypto.generateKey") else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let extractable = extractable.0;
    let Some(promise) = PendingCryptoPromise::new(scope, &mut rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };

    let Some(name) = crypto_algorithm_name(scope, args.get(0)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let Ok(name) = name.parse::<WebCryptoKeyAlgorithm>() else {
        promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
        return;
    };

    match name {
        WebCryptoKeyAlgorithm::AesCbc
        | WebCryptoKeyAlgorithm::AesCtr
        | WebCryptoKeyAlgorithm::AesGcm
        | WebCryptoKeyAlgorithm::AesKw => {
            if key_usages_contain_invalid(name, "secret", &usages) {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            let Some(algorithm_object) = args.get(0).to_object(scope) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            let length_bits = match aes_key_length_bits(scope, algorithm_object, "AesKeyGenParams")
            {
                Ok(length_bits) => length_bits,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            if usages.is_empty() {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            let key_bytes = match generate_aes_key(length_bits) {
                Ok(bytes) => bytes,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            };
            let algorithm = build_symmetric_algorithm_object(
                scope,
                webcrypto_algorithm_display_name(name),
                length_bits,
            );
            let Some(key) =
                new_crypto_key_object(scope, "secret", algorithm, extractable, &usages, &key_bytes)
            else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            promise.resolve(scope, key.into());
        }
        WebCryptoKeyAlgorithm::Chacha20Poly1305 => {
            if key_usages_contain_invalid(name, "secret", &usages) {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            if usages.is_empty() {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            let key_bytes = match generate_chacha20_poly1305_key() {
                Ok(bytes) => bytes,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            };
            let algorithm =
                build_named_algorithm_object(scope, webcrypto_algorithm_display_name(name));
            let Some(key) =
                new_crypto_key_object(scope, "secret", algorithm, extractable, &usages, &key_bytes)
            else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            promise.resolve(scope, key.into());
        }
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
        }
        WebCryptoKeyAlgorithm::Hmac => {
            let Some(algorithm_object) = args.get(0).to_object(scope) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            let hash = match required_hmac_hash_algorithm(scope, algorithm_object) {
                Ok(hash) => hash,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            if key_usages_contain_invalid(WebCryptoKeyAlgorithm::Hmac, "secret", &usages) {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            let length_bits = match hmac_generate_key_length_bits(scope, algorithm_object, hash) {
                Ok(length_bits) => length_bits,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            if usages.is_empty() {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            let key_bytes = match generate_hmac_key(hash, Some(length_bits)) {
                Ok(bytes) => bytes,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            };
            let algorithm = build_hmac_algorithm_object(scope, hash.as_ref(), length_bits);
            let Some(key) =
                new_crypto_key_object(scope, "secret", algorithm, extractable, &usages, &key_bytes)
            else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            promise.resolve(scope, key.into());
        }
        WebCryptoKeyAlgorithm::RsaOaep
        | WebCryptoKeyAlgorithm::RsaPss
        | WebCryptoKeyAlgorithm::RsassaPkcs1V15 => {
            let Some(algorithm_object) = args.get(0).to_object(scope) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            // NormalizeAlgorithm validates RSA key-generation members before
            // checking whether the requested usages are compatible with the
            // resolved algorithm. WPT relies on this ordering for invalid
            // hashes such as `{ name: "RSA-PSS", hash: "SHA", ... }`, which
            // must surface as NotSupportedError even when the usage list would
            // later be invalid for RSA-PSS.
            let (modulus_length_bits, public_exponent, hash) =
                match rsa_key_gen_params(scope, algorithm_object) {
                    Ok(params) => params,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                };
            if key_pair_usages_contain_invalid(name, &usages) || usages.is_empty() {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                let usages = usages.clone();
                handle.spawn_blocking(move || {
                    let result = generate_rsa_key_pair(modulus_length_bits, &public_exponent)
                        .map(|key_pair| {
                            let (private_key, public_key) = generated_rsa_key_pair_payloads(
                                name,
                                hash,
                                extractable,
                                &usages,
                                key_pair,
                            );
                            WebCryptoTaskResult::CryptoKeyPair {
                                private_key: Box::new(private_key),
                                public_key: Box::new(public_key),
                            }
                        })
                        .map_err(WebCryptoRejection::from);
                    completion_tx.send(result);
                });
                return;
            }
            let key_pair = match generate_rsa_key_pair(modulus_length_bits, &public_exponent) {
                Ok(key_pair) => key_pair,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            };
            let (private_key, public_key) =
                generated_rsa_key_pair_payloads(name, hash, extractable, &usages, key_pair);
            resolve_crypto_key_pair_payloads(scope, &promise, private_key, public_key);
        }
        WebCryptoKeyAlgorithm::Ecdh | WebCryptoKeyAlgorithm::Ecdsa => {
            let Some(algorithm_object) = args.get(0).to_object(scope) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            // EC named-curve normalization is part of NormalizeAlgorithm and
            // must run before semantic usage checks. This keeps invalid curves
            // such as P-512 observable as NotSupportedError even when usages
            // are empty.
            let curve = match ec_named_curve_param(scope, algorithm_object) {
                Ok(curve) => curve,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            if key_pair_usages_contain_invalid(name, &usages) || usages.is_empty() {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                let usages = usages.clone();
                spawn_webcrypto_key_pair_task(handle, completion_tx, move || {
                    generate_ec_key_pair(curve).map(|key_pair| {
                        generated_ec_key_pair_payloads(name, curve, extractable, &usages, key_pair)
                    })
                });
                return;
            }
            let key_pair = match generate_ec_key_pair(curve) {
                Ok(key_pair) => key_pair,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            };
            let (private_key, public_key) =
                generated_ec_key_pair_payloads(name, curve, extractable, &usages, key_pair);
            resolve_crypto_key_pair_payloads(scope, &promise, private_key, public_key);
        }
        WebCryptoKeyAlgorithm::Ed25519
        | WebCryptoKeyAlgorithm::Ed448
        | WebCryptoKeyAlgorithm::X448 => {
            if key_pair_usages_contain_invalid(name, &usages) || usages.is_empty() {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            let curve = match okp_curve_for_algorithm(name) {
                Some(curve) => curve,
                None => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
                    return;
                }
            };
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                let usages = usages.clone();
                spawn_webcrypto_key_pair_task(handle, completion_tx, move || {
                    generate_okp_key_pair(curve).map(|key_pair| {
                        generated_okp_key_pair_payloads(name, extractable, &usages, key_pair)
                    })
                });
                return;
            }
            let key_pair = match generate_okp_key_pair(curve) {
                Ok(key_pair) => key_pair,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            };
            let (private_key, public_key) =
                generated_okp_key_pair_payloads(name, extractable, &usages, key_pair);
            resolve_crypto_key_pair_payloads(scope, &promise, private_key, public_key);
        }
        WebCryptoKeyAlgorithm::X25519 => {
            if key_usages_contain_invalid(WebCryptoKeyAlgorithm::X25519, "private", &usages) {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            if usages.is_empty() {
                promise.reject_webcrypto(scope, WebCryptoRejection::Syntax);
                return;
            }
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                let usages = usages.clone();
                spawn_webcrypto_key_pair_task(handle, completion_tx, move || {
                    generate_x25519_key_pair().map(|key_pair| {
                        generated_x25519_key_pair_payloads(extractable, &usages, key_pair)
                    })
                });
                return;
            }
            let key_pair = match generate_x25519_key_pair() {
                Ok(key_pair) => key_pair,
                Err(_) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::Operation);
                    return;
                }
            };
            let (private_key, public_key) =
                generated_x25519_key_pair_payloads(extractable, &usages, key_pair);
            resolve_crypto_key_pair_payloads(scope, &promise, private_key, public_key);
        }
    }
}

pub(crate) fn crypto_subtle_sign_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, &args, &mut rv) {
        return;
    }
    let Ok(key) = subtle_crypto_key_arg(scope, &args, 1) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Some(promise) = PendingCryptoPromise::new(scope, &mut rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let data_preflight = buffer_source_value_can_be_detached_to_empty(args.get(2));
    let Some(name) = crypto_algorithm_name(scope, args.get(0)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let Ok(algorithm) = name.parse::<WebCryptoKeyAlgorithm>() else {
        promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
        return;
    };
    macro_rules! operation_data {
        () => {
            match subtle_buffer_source_arg_with_max_rejection(
                scope,
                &args,
                2,
                "SubtleCrypto.sign",
                MAX_SIGNATURE_OPERATION_BYTES,
                data_preflight,
            ) {
                Ok(data) => data,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            }
        };
    }
    let signature = match algorithm {
        WebCryptoKeyAlgorithm::Hmac => {
            if crypto_key_kind(scope, key).as_deref() != Some("secret")
                || !crypto_key_has_usage(scope, key, "sign")
            {
                promise.reject_webcrypto(scope, WebCryptoRejection::InvalidAccess);
                return;
            }
            let Some(hash) = crypto_key_hmac_hash_algorithm(scope, key) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            let Some(key_bytes) = crypto_key_bytes(scope, key) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            let data = operation_data!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bytes_task(handle, completion_tx, move || {
                    hmac_signature(hash, &key_bytes, &data).ok_or(WebCryptoError::Operation)
                });
                return;
            }
            let Some(signature) = hmac_signature(hash, &key_bytes, &data) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Operation);
                return;
            };
            signature
        }
        WebCryptoKeyAlgorithm::RsassaPkcs1V15 => {
            let (key_bytes, hash) =
                match signing_rsa_key_material(scope, key, WebCryptoKeyAlgorithm::RsassaPkcs1V15) {
                    Ok(material) => material,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                };
            let data = operation_data!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bytes_task(handle, completion_tx, move || {
                    rsa_pkcs1_sign(&key_bytes, hash, &data)
                });
                return;
            }
            match rsa_pkcs1_sign(&key_bytes, hash, &data) {
                Ok(signature) => signature,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            }
        }
        WebCryptoKeyAlgorithm::RsaPss => {
            let salt_length = match rsa_pss_salt_length_rejection(scope, args.get(0)) {
                Ok(length) => length,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let (key_bytes, hash) =
                match signing_rsa_key_material(scope, key, WebCryptoKeyAlgorithm::RsaPss) {
                    Ok(material) => material,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                };
            let data = operation_data!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bytes_task(handle, completion_tx, move || {
                    rsa_pss_sign(&key_bytes, hash, salt_length, &data)
                });
                return;
            }
            match rsa_pss_sign(&key_bytes, hash, salt_length, &data) {
                Ok(signature) => signature,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            }
        }
        WebCryptoKeyAlgorithm::Ecdsa => {
            let hash = match ecdsa_operation_hash_rejection(scope, args.get(0)) {
                Ok(hash) => hash,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let (key_bytes, curve) = match signing_ec_key_material(scope, key) {
                Ok(material) => material,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let data = operation_data!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bytes_task(handle, completion_tx, move || {
                    ecdsa_sign(&key_bytes, curve, hash, &data)
                });
                return;
            }
            match ecdsa_sign(&key_bytes, curve, hash, &data) {
                Ok(signature) => signature,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            }
        }
        WebCryptoKeyAlgorithm::Ed25519 | WebCryptoKeyAlgorithm::Ed448 => {
            let curve = match required_okp_curve_for_algorithm_rejection(algorithm) {
                Ok(curve) => curve,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let key_bytes = match signing_okp_key_material(scope, key, algorithm) {
                Ok(bytes) => bytes,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let data = operation_data!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bytes_task(handle, completion_tx, move || {
                    eddsa_sign(curve, &key_bytes, &data)
                });
                return;
            }
            match eddsa_sign(curve, &key_bytes, &data) {
                Ok(signature) => signature,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            }
        }
        _ => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
    };
    match blob::array_buffer_from_bytes(scope, signature) {
        Some(buffer) => promise.resolve(scope, buffer.into()),
        None => promise.reject_webcrypto(scope, WebCryptoRejection::Type),
    }
}

pub(crate) fn crypto_subtle_verify_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, &args, &mut rv) {
        return;
    }
    let Ok(key) = subtle_crypto_key_arg(scope, &args, 1) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Some(promise) = PendingCryptoPromise::new(scope, &mut rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let signature_preflight = buffer_source_value_can_be_detached_to_empty(args.get(2));
    let data_preflight = buffer_source_value_can_be_detached_to_empty(args.get(3));
    let Some(name) = crypto_algorithm_name(scope, args.get(0)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let Ok(algorithm) = name.parse::<WebCryptoKeyAlgorithm>() else {
        promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
        return;
    };
    macro_rules! verification_inputs {
        () => {{
            let signature = match subtle_buffer_source_arg_with_max_rejection(
                scope,
                &args,
                2,
                "SubtleCrypto.verify",
                MAX_SIGNATURE_OPERATION_BYTES,
                signature_preflight,
            ) {
                Ok(signature) => signature,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let data = match subtle_buffer_source_arg_with_max_rejection(
                scope,
                &args,
                3,
                "SubtleCrypto.verify",
                MAX_SIGNATURE_OPERATION_BYTES,
                data_preflight,
            ) {
                Ok(data) => data,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            (signature, data)
        }};
    }
    let verified = match algorithm {
        WebCryptoKeyAlgorithm::Hmac => {
            if crypto_key_kind(scope, key).as_deref() != Some("secret")
                || !crypto_key_has_usage(scope, key, "verify")
            {
                promise.reject_webcrypto(scope, WebCryptoRejection::InvalidAccess);
                return;
            }
            let Some(hash) = crypto_key_hmac_hash_algorithm(scope, key) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            let Some(key_bytes) = crypto_key_bytes(scope, key) else {
                promise.reject_webcrypto(scope, WebCryptoRejection::Type);
                return;
            };
            let (signature, data) = verification_inputs!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bool_task(handle, completion_tx, move || {
                    Ok(verify_hmac(hash, &key_bytes, &data, &signature))
                });
                return;
            }
            verify_hmac(hash, &key_bytes, &data, &signature)
        }
        WebCryptoKeyAlgorithm::RsassaPkcs1V15 => {
            let (key_bytes, hash) =
                match verifying_rsa_key_material(scope, key, WebCryptoKeyAlgorithm::RsassaPkcs1V15)
                {
                    Ok(material) => material,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                };
            let (signature, data) = verification_inputs!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bool_task(handle, completion_tx, move || {
                    rsa_pkcs1_verify(&key_bytes, hash, &data, &signature)
                });
                return;
            }
            match rsa_pkcs1_verify(&key_bytes, hash, &data, &signature) {
                Ok(verified) => verified,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            }
        }
        WebCryptoKeyAlgorithm::RsaPss => {
            let salt_length = match rsa_pss_salt_length_rejection(scope, args.get(0)) {
                Ok(length) => length,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let (key_bytes, hash) =
                match verifying_rsa_key_material(scope, key, WebCryptoKeyAlgorithm::RsaPss) {
                    Ok(material) => material,
                    Err(rejection) => {
                        promise.reject_webcrypto(scope, rejection);
                        return;
                    }
                };
            let (signature, data) = verification_inputs!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bool_task(handle, completion_tx, move || {
                    rsa_pss_verify(&key_bytes, hash, salt_length, &data, &signature)
                });
                return;
            }
            match rsa_pss_verify(&key_bytes, hash, salt_length, &data, &signature) {
                Ok(verified) => verified,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            }
        }
        WebCryptoKeyAlgorithm::Ecdsa => {
            let hash = match ecdsa_operation_hash_rejection(scope, args.get(0)) {
                Ok(hash) => hash,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let (key_bytes, curve) = match verifying_ec_key_material(scope, key) {
                Ok(material) => material,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let (signature, data) = verification_inputs!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bool_task(handle, completion_tx, move || {
                    ecdsa_verify(&key_bytes, curve, hash, &data, &signature)
                });
                return;
            }
            match ecdsa_verify(&key_bytes, curve, hash, &data, &signature) {
                Ok(verified) => verified,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            }
        }
        WebCryptoKeyAlgorithm::Ed25519 | WebCryptoKeyAlgorithm::Ed448 => {
            let curve = match required_okp_curve_for_algorithm_rejection(algorithm) {
                Ok(curve) => curve,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let key_bytes = match verifying_okp_key_material(scope, key, algorithm) {
                Ok(bytes) => bytes,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
            let (signature, data) = verification_inputs!();
            if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                spawn_webcrypto_bool_task(handle, completion_tx, move || {
                    eddsa_verify(curve, &key_bytes, &data, &signature)
                });
                return;
            }
            match eddsa_verify(curve, &key_bytes, &data, &signature) {
                Ok(verified) => verified,
                Err(error) => {
                    promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
                    return;
                }
            }
        }
        _ => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
    };
    promise.resolve(scope, v8::Boolean::new(scope, verified).into());
}

pub(crate) fn crypto_subtle_get_public_key_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, &args, &mut rv) {
        return;
    }
    let Ok(key) = subtle_crypto_key_arg(scope, &args, 0) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(usages) = subtle_key_usages_arg(scope, &args, 1, "SubtleCrypto.getPublicKey") else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Some(key_type) = crypto_key_kind(scope, key) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };
    let Some(algorithm) = crypto_key_algorithm_name(scope, key)
        .and_then(|name| name.parse::<WebCryptoKeyAlgorithm>().ok())
    else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::NotSupported);
        return;
    };
    if !get_public_key_algorithm_is_supported(algorithm) {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::NotSupported);
        return;
    }
    if key_type != "private" {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::InvalidAccess);
        return;
    }
    if !key_usages_are_valid(algorithm, "public", &usages) {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Syntax);
        return;
    }
    let Some(private_key) = crypto_key_clone_payload_from_object(scope, key) else {
        set_rejected_webcrypto_promise(scope, &mut rv, WebCryptoRejection::Type);
        return;
    };

    let Some(promise) = PendingCryptoPromise::new(scope, &mut rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
        spawn_webcrypto_key_task(handle, completion_tx, move || {
            public_key_payload_from_private_payload(private_key, algorithm, usages)
        });
        return;
    }

    match public_key_payload_from_private_payload(private_key, algorithm, usages) {
        Ok(payload) => resolve_crypto_key_payload(scope, &promise, payload),
        Err(rejection) => promise.reject_webcrypto(scope, rejection),
    }
}

pub(crate) fn public_key_payload_from_private_payload(
    mut private_key: CryptoKeyClonePayload,
    algorithm: WebCryptoKeyAlgorithm,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let public_key = match algorithm {
        WebCryptoKeyAlgorithm::RsaOaep
        | WebCryptoKeyAlgorithm::RsaPss
        | WebCryptoKeyAlgorithm::RsassaPkcs1V15 => {
            rsa_public_key_from_private(&private_key.key_bytes).map_err(WebCryptoRejection::from)?
        }
        WebCryptoKeyAlgorithm::Ecdh | WebCryptoKeyAlgorithm::Ecdsa => {
            let curve = private_key
                .algorithm
                .named_curve
                .as_deref()
                .ok_or(WebCryptoRejection::Type)
                .and_then(|curve| parse_ec_named_curve(curve).ok_or(WebCryptoRejection::Type))?;
            ec_public_key_from_private(&private_key.key_bytes, curve)
                .map_err(WebCryptoRejection::from)?
        }
        WebCryptoKeyAlgorithm::Ed25519 | WebCryptoKeyAlgorithm::Ed448 => {
            let curve = required_okp_curve_for_algorithm_rejection(algorithm)?;
            okp_public_key_from_private(curve, &private_key.key_bytes)
                .map_err(WebCryptoRejection::from)?
        }
        WebCryptoKeyAlgorithm::X25519 => {
            let private_bytes = <[u8; 32]>::try_from(private_key.key_bytes.as_slice())
                .map_err(|_| WebCryptoRejection::Type)?;
            x25519_public_key_from_private(&private_bytes)
                .map(|public_key| public_key.to_vec())
                .map_err(WebCryptoRejection::from)?
        }
        WebCryptoKeyAlgorithm::X448 => {
            okp_public_key_from_private(WebCryptoOkpCurve::X448, &private_key.key_bytes)
                .map_err(WebCryptoRejection::from)?
        }
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    private_key.key_type = "public".to_owned();
    private_key.extractable = true;
    private_key.usages = usages;
    private_key.key_bytes = public_key;
    Ok(private_key)
}

pub(crate) fn crypto_subtle_supports_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(operation) =
        subtle_required_arg::<webidl::DomString>(scope, &args, 0, "SubtleCrypto.supports")
    else {
        webidl::throw_type_error(scope, "SubtleCrypto.supports requires an operation");
        return;
    };
    if args.length() <= 1 {
        webidl::throw_type_error(scope, "SubtleCrypto.supports requires an algorithm");
        return;
    }
    let extra = (args.length() > 2).then(|| args.get(2));
    let supported = crypto_subtle_supports_operation(scope, &operation.0, args.get(1), extra);
    rv.set(v8::Boolean::new(scope, supported).into());
}

pub(crate) fn crypto_subtle_supports_operation<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    operation: &str,
    algorithm_value: v8::Local<'s, v8::Value>,
    extra: Option<v8::Local<'s, v8::Value>>,
) -> bool {
    let Some(name) = crypto_algorithm_name(scope, algorithm_value) else {
        return false;
    };
    if operation == "digest" {
        return name.parse::<WebCryptoHashAlgorithm>().is_ok();
    }
    let Ok(algorithm) = name.parse::<WebCryptoKeyAlgorithm>() else {
        return false;
    };
    match operation {
        "generateKey" => supports_generate_key(scope, algorithm_value, algorithm),
        "importKey" => matches!(
            algorithm,
            WebCryptoKeyAlgorithm::AesCbc
                | WebCryptoKeyAlgorithm::AesCtr
                | WebCryptoKeyAlgorithm::AesGcm
                | WebCryptoKeyAlgorithm::AesKw
                | WebCryptoKeyAlgorithm::Chacha20Poly1305
                | WebCryptoKeyAlgorithm::Hkdf
                | WebCryptoKeyAlgorithm::Hmac
                | WebCryptoKeyAlgorithm::Pbkdf2
                | WebCryptoKeyAlgorithm::RsaOaep
                | WebCryptoKeyAlgorithm::RsaPss
                | WebCryptoKeyAlgorithm::RsassaPkcs1V15
                | WebCryptoKeyAlgorithm::Ecdh
                | WebCryptoKeyAlgorithm::Ecdsa
                | WebCryptoKeyAlgorithm::Ed25519
                | WebCryptoKeyAlgorithm::Ed448
                | WebCryptoKeyAlgorithm::X25519
                | WebCryptoKeyAlgorithm::X448
        ),
        "exportKey" => matches!(
            algorithm,
            WebCryptoKeyAlgorithm::AesCbc
                | WebCryptoKeyAlgorithm::AesCtr
                | WebCryptoKeyAlgorithm::AesGcm
                | WebCryptoKeyAlgorithm::AesKw
                | WebCryptoKeyAlgorithm::Chacha20Poly1305
                | WebCryptoKeyAlgorithm::Hmac
                | WebCryptoKeyAlgorithm::RsaOaep
                | WebCryptoKeyAlgorithm::RsaPss
                | WebCryptoKeyAlgorithm::RsassaPkcs1V15
                | WebCryptoKeyAlgorithm::Ecdh
                | WebCryptoKeyAlgorithm::Ecdsa
                | WebCryptoKeyAlgorithm::Ed25519
                | WebCryptoKeyAlgorithm::Ed448
                | WebCryptoKeyAlgorithm::X25519
                | WebCryptoKeyAlgorithm::X448
        ),
        "sign" | "verify" => matches!(
            algorithm,
            WebCryptoKeyAlgorithm::Hmac
                | WebCryptoKeyAlgorithm::RsaPss
                | WebCryptoKeyAlgorithm::RsassaPkcs1V15
                | WebCryptoKeyAlgorithm::Ecdsa
                | WebCryptoKeyAlgorithm::Ed25519
                | WebCryptoKeyAlgorithm::Ed448
        ),
        "encrypt" | "decrypt" => {
            if algorithm == WebCryptoKeyAlgorithm::RsaOaep {
                supports_rsa_oaep_operation(scope, algorithm_value)
            } else if algorithm == WebCryptoKeyAlgorithm::Chacha20Poly1305 {
                normalize_symmetric_cipher_algorithm_rejection(scope, algorithm_value).is_ok()
            } else {
                supports_aes_cipher_operation(scope, algorithm_value, algorithm)
            }
        }
        "wrapKey" | "unwrapKey" => {
            if algorithm == WebCryptoKeyAlgorithm::RsaOaep {
                supports_rsa_oaep_operation(scope, algorithm_value)
            } else {
                validate_symmetric_wrapping_algorithm_rejection(algorithm)
                    .and_then(|algorithm| {
                        normalize_symmetric_wrapping_params_rejection(
                            scope,
                            algorithm_value,
                            algorithm,
                        )
                    })
                    .is_ok()
            }
        }
        "deriveBits" => supports_derive_bits(scope, algorithm_value, algorithm, extra),
        "deriveKey" => supports_derive_key(scope, algorithm_value, algorithm, extra),
        "getPublicKey" => get_public_key_algorithm_is_supported(algorithm),
        _ => false,
    }
}

pub(crate) fn get_public_key_algorithm_is_supported(algorithm: WebCryptoKeyAlgorithm) -> bool {
    matches!(
        algorithm,
        WebCryptoKeyAlgorithm::RsaOaep
            | WebCryptoKeyAlgorithm::RsaPss
            | WebCryptoKeyAlgorithm::RsassaPkcs1V15
            | WebCryptoKeyAlgorithm::Ecdh
            | WebCryptoKeyAlgorithm::Ecdsa
            | WebCryptoKeyAlgorithm::Ed25519
            | WebCryptoKeyAlgorithm::Ed448
            | WebCryptoKeyAlgorithm::X25519
            | WebCryptoKeyAlgorithm::X448
    )
}

pub(crate) fn supports_generate_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm_value: v8::Local<'s, v8::Value>,
    algorithm: WebCryptoKeyAlgorithm,
) -> bool {
    match algorithm {
        WebCryptoKeyAlgorithm::AesCbc
        | WebCryptoKeyAlgorithm::AesCtr
        | WebCryptoKeyAlgorithm::AesGcm
        | WebCryptoKeyAlgorithm::AesKw => algorithm_value
            .to_object(scope)
            .is_some_and(|object| aes_key_length_bits(scope, object, "AesKeyGenParams").is_ok()),
        WebCryptoKeyAlgorithm::Chacha20Poly1305 => true,
        WebCryptoKeyAlgorithm::Hmac => supports_hmac_key_generation(scope, algorithm_value),
        WebCryptoKeyAlgorithm::RsaOaep
        | WebCryptoKeyAlgorithm::RsaPss
        | WebCryptoKeyAlgorithm::RsassaPkcs1V15 => algorithm_value
            .to_object(scope)
            .is_some_and(|object| rsa_key_gen_params(scope, object).is_ok()),
        WebCryptoKeyAlgorithm::Ecdh | WebCryptoKeyAlgorithm::Ecdsa => algorithm_value
            .to_object(scope)
            .is_some_and(|object| ec_named_curve_param(scope, object).is_ok()),
        WebCryptoKeyAlgorithm::Ed25519
        | WebCryptoKeyAlgorithm::Ed448
        | WebCryptoKeyAlgorithm::X448 => true,
        WebCryptoKeyAlgorithm::X25519 => true,
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => false,
    }
}

pub(crate) fn supports_hmac_key_generation<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm_value: v8::Local<'s, v8::Value>,
) -> bool {
    let Some(algorithm_object) = algorithm_value.to_object(scope) else {
        return false;
    };
    let Ok(hash) = required_hmac_hash_algorithm(scope, algorithm_object) else {
        return false;
    };
    hmac_generate_key_length_bits(scope, algorithm_object, hash).is_ok()
}

pub(crate) fn supports_aes_cipher_operation<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm_value: v8::Local<'s, v8::Value>,
    algorithm: WebCryptoKeyAlgorithm,
) -> bool {
    normalize_symmetric_cipher_algorithm_rejection(scope, algorithm_value)
        .is_ok_and(|params| params.algorithm == algorithm)
}

pub(crate) fn supports_rsa_oaep_operation<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm_value: v8::Local<'s, v8::Value>,
) -> bool {
    rsa_oaep_operation_label_rejection(scope, algorithm_value).is_ok()
}

pub(crate) fn supports_derive_bits<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm_value: v8::Local<'s, v8::Value>,
    algorithm: WebCryptoKeyAlgorithm,
    length_value: Option<v8::Local<'s, v8::Value>>,
) -> bool {
    match algorithm {
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => {
            let Some(algorithm_object) = algorithm_value.to_object(scope) else {
                return false;
            };
            let Ok(params) = kdf_derive_params(scope, algorithm_object, algorithm) else {
                return false;
            };
            supports_kdf_derive_bits_length(scope, length_value, algorithm, params.hash)
        }
        WebCryptoKeyAlgorithm::X25519 => {
            supports_derive_public_parameter(scope, algorithm_value, WebCryptoKeyAlgorithm::X25519)
                .is_some_and(|max_bits| {
                    supports_derive_bits_length(scope, length_value, false, Some(max_bits))
                })
        }
        WebCryptoKeyAlgorithm::X448 => {
            supports_derive_public_parameter(scope, algorithm_value, WebCryptoKeyAlgorithm::X448)
                .is_some_and(|max_bits| {
                    supports_derive_bits_length(scope, length_value, false, Some(max_bits))
                })
        }
        WebCryptoKeyAlgorithm::Ecdh => {
            supports_derive_public_parameter(scope, algorithm_value, WebCryptoKeyAlgorithm::Ecdh)
                .is_some_and(|max_bits| {
                    supports_derive_bits_length(scope, length_value, false, Some(max_bits))
                })
        }
        _ => false,
    }
}

pub(crate) fn supports_derive_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm_value: v8::Local<'s, v8::Value>,
    algorithm: WebCryptoKeyAlgorithm,
    target_value: Option<v8::Local<'s, v8::Value>>,
) -> bool {
    let source_max_bits = match algorithm {
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => {
            algorithm_value.to_object(scope).and_then(|object| {
                kdf_derive_params(scope, object, algorithm)
                    .is_ok()
                    .then_some(None)
            })
        }
        WebCryptoKeyAlgorithm::X25519 => {
            supports_derive_public_parameter(scope, algorithm_value, WebCryptoKeyAlgorithm::X25519)
                .map(Some)
        }
        WebCryptoKeyAlgorithm::X448 => {
            supports_derive_public_parameter(scope, algorithm_value, WebCryptoKeyAlgorithm::X448)
                .map(Some)
        }
        WebCryptoKeyAlgorithm::Ecdh => {
            supports_derive_public_parameter(scope, algorithm_value, WebCryptoKeyAlgorithm::Ecdh)
                .map(Some)
        }
        _ => None,
    };
    source_max_bits.is_some_and(|max_bits| {
        target_value
            .is_some_and(|target| supports_derived_key_target(scope, target, algorithm, max_bits))
    })
}

pub(crate) fn supports_derive_bits_length<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    length_value: Option<v8::Local<'s, v8::Value>>,
    required: bool,
    max_bits: Option<usize>,
) -> bool {
    let Some(value) = length_value else {
        return !required;
    };
    if value.is_null_or_undefined() {
        return !required;
    }
    let Ok(length) = webidl::convert::<webidl::UnsignedLong>(
        scope,
        value,
        webidl::Context::argument("SubtleCrypto.supports", 3),
    ) else {
        return false;
    };
    max_bits.is_none_or(|max_bits| length.0 as usize <= max_bits)
}

pub(crate) fn supports_kdf_derive_bits_length<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    length_value: Option<v8::Local<'s, v8::Value>>,
    algorithm: WebCryptoKeyAlgorithm,
    hash: WebCryptoHashAlgorithm,
) -> bool {
    // HKDF and PBKDF2 materialize bytes, so their reported support must follow
    // the same byte-alignment and backend output-limit checks as deriveBits().
    // X25519 is handled separately because Chromium accepts omitted/null and
    // non-byte-aligned lengths for that source algorithm.
    let Some(value) = length_value else {
        return false;
    };
    if value.is_null_or_undefined() {
        return false;
    }
    let Ok(length) = webidl::convert::<webidl::UnsignedLong>(
        scope,
        value,
        webidl::Context::argument("SubtleCrypto.supports", 3),
    ) else {
        return false;
    };
    let length_bits = length.0 as usize;
    if !length_bits.is_multiple_of(8) {
        return false;
    }
    if length_bits > MAX_KDF_DERIVED_BITS {
        return false;
    }
    match algorithm {
        WebCryptoKeyAlgorithm::Hkdf => length_bits / 8 <= 255 * hash.output_len_bytes(),
        WebCryptoKeyAlgorithm::Pbkdf2 => true,
        _ => false,
    }
}

pub(crate) fn supports_derive_public_parameter<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    algorithm_value: v8::Local<'s, v8::Value>,
    algorithm: WebCryptoKeyAlgorithm,
) -> Option<usize> {
    let algorithm_object = algorithm_value.to_object(scope)?;
    let public_key = algorithm_object
        .get(scope, v8str(scope, "public").into())
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())?;
    if crypto_key_kind(scope, public_key).as_deref() != Some("public")
        || crypto_key_algorithm_name(scope, public_key).as_deref()
            != Some(crypto_algorithm_name_for_match(algorithm))
    {
        return None;
    }
    match algorithm {
        WebCryptoKeyAlgorithm::X25519 => Some(256),
        WebCryptoKeyAlgorithm::X448 => Some(448),
        WebCryptoKeyAlgorithm::Ecdh => crypto_key_ec_named_curve(scope, public_key)
            .map(|curve| curve.coordinate_len_bytes() * 8),
        _ => None,
    }
}

pub(crate) fn supports_derived_key_target<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target_value: v8::Local<'s, v8::Value>,
    source_algorithm: WebCryptoKeyAlgorithm,
    source_max_bits: Option<usize>,
) -> bool {
    let Some(target_name) = crypto_algorithm_name(scope, target_value) else {
        return false;
    };
    let Ok(target_algorithm) = target_name.parse::<WebCryptoKeyAlgorithm>() else {
        return false;
    };
    let Some(target_object) = target_value.to_object(scope) else {
        return false;
    };
    match target_algorithm {
        WebCryptoKeyAlgorithm::AesCbc
        | WebCryptoKeyAlgorithm::AesCtr
        | WebCryptoKeyAlgorithm::AesGcm
        | WebCryptoKeyAlgorithm::AesKw => {
            aes_key_length_bits(scope, target_object, "AesDerivedKeyParams").is_ok()
        }
        WebCryptoKeyAlgorithm::Hmac => target_object
            .get(scope, v8str(scope, "hash").into())
            .and_then(|value| crypto_algorithm_name(scope, value))
            .and_then(|hash| hash.parse::<WebCryptoHashAlgorithm>().ok())
            .and_then(|hash| hmac_derived_key_length_bits(scope, target_object, hash).ok())
            .is_some_and(|length_bits| {
                // HKDF/PBKDF2 derive bytes and reject non-byte lengths. ECDH
                // and X* derive from a fixed-size shared secret and may then
                // truncate to arbitrary bit lengths, so the derived target must
                // fit inside that source secret.
                if let Some(max_bits) = source_max_bits {
                    length_bits <= max_bits
                } else {
                    length_bits.is_multiple_of(8)
                }
            }),
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => {
            matches!(
                source_algorithm,
                WebCryptoKeyAlgorithm::X25519
                    | WebCryptoKeyAlgorithm::X448
                    | WebCryptoKeyAlgorithm::Ecdh
            )
        }
        WebCryptoKeyAlgorithm::X25519
        | WebCryptoKeyAlgorithm::X448
        | WebCryptoKeyAlgorithm::Chacha20Poly1305
        | WebCryptoKeyAlgorithm::RsaOaep
        | WebCryptoKeyAlgorithm::RsaPss
        | WebCryptoKeyAlgorithm::RsassaPkcs1V15
        | WebCryptoKeyAlgorithm::Ecdh
        | WebCryptoKeyAlgorithm::Ecdsa
        | WebCryptoKeyAlgorithm::Ed25519
        | WebCryptoKeyAlgorithm::Ed448 => false,
    }
}
