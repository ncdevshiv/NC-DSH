use super::*;

pub(crate) fn crypto_subtle_encrypt_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    crypto_subtle_cipher_operation_callback(
        scope,
        &args,
        &mut rv,
        "SubtleCrypto.encrypt",
        "encrypt",
    );
}

pub(crate) fn crypto_subtle_decrypt_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    crypto_subtle_cipher_operation_callback(
        scope,
        &args,
        &mut rv,
        "SubtleCrypto.decrypt",
        "decrypt",
    );
}

pub(crate) fn crypto_subtle_wrap_key_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    crypto_subtle_wrap_operation_callback(scope, &args, &mut rv);
}

pub(crate) fn crypto_subtle_unwrap_key_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    crypto_subtle_unwrap_operation_callback(scope, &args, &mut rv);
}

pub(crate) fn crypto_subtle_cipher_operation_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
    context: &'static str,
    required_usage: &'static str,
) {
    if !subtle_crypto_receiver_is_valid(scope, args, rv) {
        return;
    }
    let Ok(key) = subtle_crypto_key_arg(scope, args, 1) else {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    };
    let data_preflight = buffer_source_value_can_be_detached_to_empty(args.get(2));
    let Some(promise) = PendingCryptoPromise::new(scope, rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let Some(name) = crypto_algorithm_name(scope, args.get(0)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let Ok(algorithm_name) = name.parse::<WebCryptoKeyAlgorithm>() else {
        promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
        return;
    };
    if algorithm_name == WebCryptoKeyAlgorithm::RsaOaep {
        let label = match rsa_oaep_operation_label_rejection(scope, args.get(0)) {
            Ok(label) => label,
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        };
        let (key_bytes, hash) = match rsa_oaep_key_material_rejection(scope, key, required_usage) {
            Ok(material) => material,
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        };
        let data = match subtle_buffer_source_arg_with_max_rejection(
            scope,
            args,
            2,
            context,
            MAX_CIPHER_OPERATION_BYTES,
            data_preflight,
        ) {
            Ok(data) => data,
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        };
        let result = match required_usage {
            "encrypt" => {
                if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                    spawn_webcrypto_bytes_task(handle, completion_tx, move || {
                        rsa_oaep_encrypt(&key_bytes, hash, &label, &data)
                    });
                    return;
                }
                rsa_oaep_encrypt(&key_bytes, hash, &label, &data)
            }
            "decrypt" => {
                if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
                    spawn_webcrypto_bytes_task(handle, completion_tx, move || {
                        rsa_oaep_decrypt(&key_bytes, hash, &label, &data)
                    });
                    return;
                }
                rsa_oaep_decrypt(&key_bytes, hash, &label, &data)
            }
            _ => Err(WebCryptoError::Operation),
        };
        match result {
            Ok(bytes) => match blob::array_buffer_from_bytes(scope, bytes) {
                Some(buffer) => promise.resolve(scope, buffer.into()),
                None => promise.reject_webcrypto(scope, WebCryptoRejection::Type),
            },
            Err(error) => promise.reject_webcrypto(scope, WebCryptoRejection::from(error)),
        }
        return;
    }
    let algorithm = match algorithm_name {
        WebCryptoKeyAlgorithm::AesCbc
        | WebCryptoKeyAlgorithm::AesCtr
        | WebCryptoKeyAlgorithm::AesGcm
        | WebCryptoKeyAlgorithm::Chacha20Poly1305 => {
            match normalize_symmetric_operation_params_rejection(scope, args.get(0), algorithm_name)
            {
                Ok(params) => NormalizedCipherAlgorithm {
                    algorithm: algorithm_name,
                    params,
                },
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            }
        }
        _ => {
            promise.reject_webcrypto(scope, WebCryptoRejection::NotSupported);
            return;
        }
    };
    if !crypto_key_can_run_symmetric_operation(scope, key, algorithm.algorithm, required_usage) {
        promise.reject_webcrypto(scope, WebCryptoRejection::InvalidAccess);
        return;
    }
    let Some(key_bytes) = crypto_key_bytes(scope, key) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    // WebCrypto normalizes algorithm dictionaries before converting operation
    // BufferSource data. WPT relies on this for getters that mutate or detach
    // the plaintext/ciphertext during the call.
    let data = match subtle_buffer_source_arg_with_max_rejection(
        scope,
        args,
        2,
        context,
        MAX_CIPHER_OPERATION_BYTES,
        data_preflight,
    ) {
        Ok(data) => data,
        Err(rejection) => {
            promise.reject_webcrypto(scope, rejection);
            return;
        }
    };
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
        let params = algorithm.params;
        spawn_webcrypto_bytes_task(handle, completion_tx, move || {
            run_symmetric_cipher_operation(required_usage, &params, &key_bytes, &data)
        });
        return;
    }
    match run_symmetric_cipher_operation(required_usage, &algorithm.params, &key_bytes, &data) {
        Ok(bytes) => match blob::array_buffer_from_bytes(scope, bytes) {
            Some(buffer) => promise.resolve(scope, buffer.into()),
            None => promise.reject_webcrypto(scope, WebCryptoRejection::Type),
        },
        Err(error) => promise.reject_webcrypto(scope, WebCryptoRejection::from(error)),
    }
}

pub(crate) fn crypto_subtle_wrap_operation_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, args, rv) {
        return;
    }
    let Ok(format) =
        subtle_required_arg::<webidl::DomString>(scope, args, 0, "SubtleCrypto.wrapKey")
    else {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    };
    let format = format.0;
    if !subtle_key_format_is_valid(&format) {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    }
    let Ok(key) = subtle_crypto_key_arg(scope, args, 1) else {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(wrapping_key) = subtle_crypto_key_arg(scope, args, 2) else {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    };
    let Some(promise) = PendingCryptoPromise::new(scope, rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let Some(wrapping_name) = crypto_algorithm_name(scope, args.get(3)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    if wrapping_name.parse::<WebCryptoKeyAlgorithm>() == Ok(WebCryptoKeyAlgorithm::RsaOaep) {
        let label = match rsa_oaep_operation_label_rejection(scope, args.get(3)) {
            Ok(label) => label,
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        };
        let (wrapping_key_bytes, hash) =
            match rsa_oaep_key_material_rejection(scope, wrapping_key, "wrapKey") {
                Ok(material) => material,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
        let target_snapshot = match snapshot_key_material_for_wrap_rejection(scope, key, &format) {
            Ok(snapshot) => snapshot,
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        };
        if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
            spawn_webcrypto_result_task(handle, completion_tx, move || {
                wrap_key_task_result(target_snapshot, |key_material| {
                    rsa_oaep_encrypt(&wrapping_key_bytes, hash, &label, &key_material)
                })
            });
            return;
        }
        resolve_webcrypto_task_result(
            scope,
            &promise,
            wrap_key_task_result(target_snapshot, |key_material| {
                rsa_oaep_encrypt(&wrapping_key_bytes, hash, &label, &key_material)
            }),
        );
        return;
    }
    let wrapping_algorithm = match wrapping_name
        .parse::<WebCryptoKeyAlgorithm>()
        .map_err(|_| WebCryptoRejection::NotSupported)
        .and_then(validate_symmetric_wrapping_algorithm_rejection)
    {
        Ok(algorithm) => algorithm,
        Err(rejection) => {
            promise.reject_webcrypto(scope, rejection);
            return;
        }
    };
    if !crypto_key_can_run_symmetric_operation(scope, wrapping_key, wrapping_algorithm, "wrapKey") {
        promise.reject_webcrypto(scope, WebCryptoRejection::InvalidAccess);
        return;
    }
    let wrapping_params =
        match normalize_symmetric_wrapping_params_rejection(scope, args.get(3), wrapping_algorithm)
        {
            Ok(params) => params,
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        };
    let target_snapshot = match snapshot_key_material_for_wrap_rejection(scope, key, &format) {
        Ok(snapshot) => snapshot,
        Err(rejection) => {
            promise.reject_webcrypto(scope, rejection);
            return;
        }
    };
    let Some(wrapping_key_bytes) = crypto_key_bytes(scope, wrapping_key) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
        spawn_webcrypto_result_task(handle, completion_tx, move || {
            wrap_key_task_result(target_snapshot, |key_material| {
                run_symmetric_cipher_operation(
                    "wrapKey",
                    &wrapping_params,
                    &wrapping_key_bytes,
                    &key_material,
                )
            })
        });
        return;
    }
    resolve_webcrypto_task_result(
        scope,
        &promise,
        wrap_key_task_result(target_snapshot, |key_material| {
            run_symmetric_cipher_operation(
                "wrapKey",
                &wrapping_params,
                &wrapping_key_bytes,
                &key_material,
            )
        }),
    );
}

pub(crate) fn crypto_subtle_unwrap_operation_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
) {
    if !subtle_crypto_receiver_is_valid(scope, args, rv) {
        return;
    }
    let Ok(format) =
        subtle_required_arg::<webidl::DomString>(scope, args, 0, "SubtleCrypto.unwrapKey")
    else {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    };
    if !subtle_key_format_is_valid(&format.0) {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    }
    // WebIDL rejects a non-BufferSource wrapped payload before the operation
    // body can report later DOMExceptions, but Blink's SubtleCrypto::unwrapKey
    // does not copy the bytes until after keyUsages and both algorithms have
    // been normalized. Keep those two phases separate so getter side effects
    // see the same ordering as Chromium.
    if !buffer_source_value_is_acceptable(args.get(1)) {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    }
    let Ok(unwrapping_key) = subtle_crypto_key_arg(scope, args, 2) else {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(extractable) =
        subtle_required_arg::<webidl::Boolean>(scope, args, 5, "SubtleCrypto.unwrapKey")
    else {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    };
    let Ok(key_usages) = subtle_key_usages_arg(scope, args, 6, "SubtleCrypto.unwrapKey") else {
        set_rejected_webcrypto_promise(scope, rv, WebCryptoRejection::Type);
        return;
    };
    let Some(promise) = PendingCryptoPromise::new(scope, rv) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let Some(unwrap_name) = crypto_algorithm_name(scope, args.get(3)) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    let import_task = match normalize_unwrapped_key_import_task_rejection(
        scope,
        &format.0,
        args.get(4),
        extractable.0,
        key_usages.clone(),
    ) {
        Ok(task) => task,
        Err(rejection) => {
            promise.reject_webcrypto(scope, rejection);
            return;
        }
    };
    let wrapped_key = match subtle_buffer_source_arg_with_max_rejection(
        scope,
        args,
        1,
        "SubtleCrypto.unwrapKey",
        MAX_CIPHER_OPERATION_BYTES,
        true,
    ) {
        Ok(bytes) => bytes,
        Err(rejection) => {
            promise.reject_webcrypto(scope, rejection);
            return;
        }
    };
    if unwrap_name.parse::<WebCryptoKeyAlgorithm>() == Ok(WebCryptoKeyAlgorithm::RsaOaep) {
        let label = match rsa_oaep_operation_label_rejection(scope, args.get(3)) {
            Ok(label) => label,
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        };
        let (unwrapping_key_bytes, hash) =
            match rsa_oaep_key_material_rejection(scope, unwrapping_key, "unwrapKey") {
                Ok(material) => material,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
        if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
            spawn_webcrypto_key_task(handle, completion_tx, move || {
                let unwrapped_bytes =
                    rsa_oaep_decrypt(&unwrapping_key_bytes, hash, &label, &wrapped_key)
                        .map_err(WebCryptoRejection::from)?;
                crypto_key_payload_from_unwrapped_material(import_task, unwrapped_bytes)
            });
            return;
        }
        let unwrapped_payload =
            match rsa_oaep_decrypt(&unwrapping_key_bytes, hash, &label, &wrapped_key)
                .map_err(WebCryptoRejection::from)
                .and_then(|bytes| crypto_key_payload_from_unwrapped_material(import_task, bytes))
            {
                Ok(payload) => payload,
                Err(rejection) => {
                    promise.reject_webcrypto(scope, rejection);
                    return;
                }
            };
        resolve_crypto_key_payload(scope, &promise, unwrapped_payload);
        return;
    }
    let unwrap_algorithm = match unwrap_name
        .parse::<WebCryptoKeyAlgorithm>()
        .map_err(|_| WebCryptoRejection::NotSupported)
        .and_then(validate_symmetric_wrapping_algorithm_rejection)
    {
        Ok(algorithm) => algorithm,
        Err(rejection) => {
            promise.reject_webcrypto(scope, rejection);
            return;
        }
    };
    if !crypto_key_can_run_symmetric_operation(scope, unwrapping_key, unwrap_algorithm, "unwrapKey")
    {
        promise.reject_webcrypto(scope, WebCryptoRejection::InvalidAccess);
        return;
    }
    let unwrap_params =
        match normalize_symmetric_wrapping_params_rejection(scope, args.get(3), unwrap_algorithm) {
            Ok(params) => params,
            Err(rejection) => {
                promise.reject_webcrypto(scope, rejection);
                return;
            }
        };
    let Some(unwrapping_key_bytes) = crypto_key_bytes(scope, unwrapping_key) else {
        promise.reject_webcrypto(scope, WebCryptoRejection::Type);
        return;
    };
    if let Some((handle, completion_tx)) = register_webcrypto_task(scope, &promise) {
        spawn_webcrypto_key_task(handle, completion_tx, move || {
            run_symmetric_cipher_operation(
                "unwrapKey",
                &unwrap_params,
                &unwrapping_key_bytes,
                &wrapped_key,
            )
            .map_err(WebCryptoRejection::from)
            .and_then(|bytes| crypto_key_payload_from_unwrapped_material(import_task, bytes))
        });
        return;
    }
    let unwrapped_bytes = match run_symmetric_cipher_operation(
        "unwrapKey",
        &unwrap_params,
        &unwrapping_key_bytes,
        &wrapped_key,
    ) {
        Ok(bytes) => bytes,
        Err(error) => {
            promise.reject_webcrypto(scope, WebCryptoRejection::from(error));
            return;
        }
    };
    match crypto_key_payload_from_unwrapped_material(import_task, unwrapped_bytes) {
        Ok(payload) => resolve_crypto_key_payload(scope, &promise, payload),
        Err(rejection) => promise.reject_webcrypto(scope, rejection),
    }
}

pub(crate) fn normalize_unwrapped_key_import_task_rejection<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    format: &str,
    algorithm: v8::Local<'s, v8::Value>,
    extractable: bool,
    usages: Vec<String>,
) -> Result<UnwrappedKeyImportTask, WebCryptoRejection> {
    let Some(name) = crypto_algorithm_name(scope, algorithm) else {
        return Err(WebCryptoRejection::Type);
    };
    let parsed = name
        .parse::<WebCryptoKeyAlgorithm>()
        .map_err(|_| WebCryptoRejection::NotSupported)?;
    let algorithm = match parsed {
        WebCryptoKeyAlgorithm::AesCbc
        | WebCryptoKeyAlgorithm::AesCtr
        | WebCryptoKeyAlgorithm::AesGcm
        | WebCryptoKeyAlgorithm::AesKw => UnwrappedKeyImportAlgorithm::Aes(parsed),
        WebCryptoKeyAlgorithm::Chacha20Poly1305 => UnwrappedKeyImportAlgorithm::Chacha20Poly1305,
        WebCryptoKeyAlgorithm::Hkdf | WebCryptoKeyAlgorithm::Pbkdf2 => {
            UnwrappedKeyImportAlgorithm::Kdf(parsed)
        }
        WebCryptoKeyAlgorithm::Hmac => {
            UnwrappedKeyImportAlgorithm::Hmac(hmac_import_params_rejection(scope, algorithm)?)
        }
        WebCryptoKeyAlgorithm::X25519 => UnwrappedKeyImportAlgorithm::X25519,
        WebCryptoKeyAlgorithm::RsaOaep
        | WebCryptoKeyAlgorithm::RsaPss
        | WebCryptoKeyAlgorithm::RsassaPkcs1V15 => UnwrappedKeyImportAlgorithm::Rsa {
            algorithm: parsed,
            hash: rsa_import_hash_rejection(scope, algorithm)?,
        },
        WebCryptoKeyAlgorithm::Ecdh | WebCryptoKeyAlgorithm::Ecdsa => {
            let Some(algorithm_object) = algorithm.to_object(scope) else {
                return Err(WebCryptoRejection::Type);
            };
            UnwrappedKeyImportAlgorithm::Ec {
                algorithm: parsed,
                curve: ec_named_curve_param(scope, algorithm_object)?,
            }
        }
        WebCryptoKeyAlgorithm::Ed25519
        | WebCryptoKeyAlgorithm::Ed448
        | WebCryptoKeyAlgorithm::X448 => UnwrappedKeyImportAlgorithm::Okp {
            algorithm: parsed,
            curve: required_okp_curve_for_algorithm_rejection(parsed)?,
        },
    };
    Ok(UnwrappedKeyImportTask {
        format: format.to_owned(),
        algorithm,
        extractable,
        usages,
    })
}

pub(crate) struct UnwrappedKeyImportTask {
    format: String,
    algorithm: UnwrappedKeyImportAlgorithm,
    extractable: bool,
    usages: Vec<String>,
}

pub(crate) enum UnwrappedKeyImportAlgorithm {
    Aes(WebCryptoKeyAlgorithm),
    Chacha20Poly1305,
    Kdf(WebCryptoKeyAlgorithm),
    Hmac(HmacImportParams),
    X25519,
    Rsa {
        algorithm: WebCryptoKeyAlgorithm,
        hash: WebCryptoHashAlgorithm,
    },
    Ec {
        algorithm: WebCryptoKeyAlgorithm,
        curve: WebCryptoEcNamedCurve,
    },
    Okp {
        algorithm: WebCryptoKeyAlgorithm,
        curve: WebCryptoOkpCurve,
    },
}

pub(crate) fn import_key_data_from_unwrapped_material(
    format: &str,
    key_bytes: Vec<u8>,
) -> Result<ImportKeyData, WebCryptoRejection> {
    Ok(match format {
        "raw" | "raw-public" | "raw-private" | "raw-seed" | "raw-secret" => {
            if key_bytes.len() > MAX_RAW_KEY_IMPORT_BYTES {
                return Err(WebCryptoRejection::Operation);
            }
            ImportKeyData::Bytes(key_bytes)
        }
        "spki" | "pkcs8" => {
            if key_bytes.len() > MAX_DER_KEY_BYTES {
                return Err(WebCryptoRejection::Operation);
            }
            ImportKeyData::Bytes(key_bytes)
        }
        "jwk" => {
            if key_bytes.len() > MAX_JWK_SERIALIZED_BYTES {
                return Err(WebCryptoRejection::Operation);
            }
            match serde_json::from_slice::<ParsedJsonWebKey>(&key_bytes) {
                Ok(jwk) => ImportKeyData::Jwk(Box::new(jwk)),
                Err(_) => return Err(WebCryptoRejection::Data),
            }
        }
        _ => return Err(WebCryptoRejection::Type),
    })
}

pub(crate) fn crypto_key_payload_from_unwrapped_material(
    task: UnwrappedKeyImportTask,
    key_bytes: Vec<u8>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let key_data = import_key_data_from_unwrapped_material(&task.format, key_bytes)?;
    match task.algorithm {
        UnwrappedKeyImportAlgorithm::Aes(algorithm) => import_aes_key_payload(
            key_data,
            &task.format,
            algorithm,
            task.extractable,
            task.usages,
        ),
        UnwrappedKeyImportAlgorithm::Kdf(algorithm) => import_kdf_key_payload(
            key_data,
            &task.format,
            algorithm,
            task.extractable,
            task.usages,
        ),
        UnwrappedKeyImportAlgorithm::Chacha20Poly1305 => import_chacha20_poly1305_key_payload(
            key_data,
            &task.format,
            task.extractable,
            task.usages,
        ),
        UnwrappedKeyImportAlgorithm::Hmac(params) => import_hmac_key_payload(
            key_data,
            &task.format,
            params,
            task.extractable,
            task.usages,
        ),
        UnwrappedKeyImportAlgorithm::X25519 => {
            import_x25519_key_payload(key_data, &task.format, task.extractable, task.usages)
        }
        UnwrappedKeyImportAlgorithm::Rsa { algorithm, hash } => import_rsa_key_payload(
            key_data,
            &task.format,
            algorithm,
            hash,
            task.extractable,
            task.usages,
        ),
        UnwrappedKeyImportAlgorithm::Ec { algorithm, curve } => import_ec_key_payload(
            key_data,
            &task.format,
            algorithm,
            curve,
            task.extractable,
            task.usages,
        ),
        UnwrappedKeyImportAlgorithm::Okp { algorithm, curve } => import_okp_key_payload(
            key_data,
            &task.format,
            algorithm,
            curve,
            task.extractable,
            task.usages,
        ),
    }
}

pub(crate) fn import_aes_key_payload(
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    extractable: bool,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let key_bytes = match format {
        "raw" | "raw-secret" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if key_usages_contain_invalid(algorithm_name, "secret", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                bytes
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => {
                if key_usages_contain_invalid(algorithm_name, "secret", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                import_aes_jwk(*value, algorithm_name, extractable, &usages)?
            }
            ImportKeyData::Bytes(_) => return Err(WebCryptoRejection::Type),
        },
        "spki" | "pkcs8" => return Err(WebCryptoRejection::NotSupported),
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    let length_bits = validate_aes_key_bytes(&key_bytes).map_err(WebCryptoRejection::from)?;
    if usages.is_empty() {
        return Err(WebCryptoRejection::Syntax);
    }
    Ok(CryptoKeyClonePayload {
        key_type: "secret".to_owned(),
        algorithm: CryptoKeyAlgorithmClonePayload {
            name: webcrypto_algorithm_display_name(algorithm_name).to_owned(),
            hash_name: None,
            length_bits: Some(length_bits),
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
        extractable,
        usages,
        key_bytes,
    })
}

pub(crate) fn import_chacha20_poly1305_key_payload(
    key_data: ImportKeyData,
    format: &str,
    extractable: bool,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let key_bytes = match format {
        "raw-secret" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if key_usages_contain_invalid(
                    WebCryptoKeyAlgorithm::Chacha20Poly1305,
                    "secret",
                    &usages,
                ) {
                    return Err(WebCryptoRejection::Syntax);
                }
                bytes
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => {
                if key_usages_contain_invalid(
                    WebCryptoKeyAlgorithm::Chacha20Poly1305,
                    "secret",
                    &usages,
                ) {
                    return Err(WebCryptoRejection::Syntax);
                }
                import_chacha20_poly1305_jwk(*value, extractable, &usages)?
            }
            ImportKeyData::Bytes(_) => return Err(WebCryptoRejection::Type),
        },
        "raw" | "spki" | "pkcs8" => return Err(WebCryptoRejection::NotSupported),
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    validate_chacha20_poly1305_key_bytes(&key_bytes).map_err(WebCryptoRejection::from)?;
    if usages.is_empty() {
        return Err(WebCryptoRejection::Syntax);
    }
    Ok(CryptoKeyClonePayload {
        key_type: "secret".to_owned(),
        algorithm: CryptoKeyAlgorithmClonePayload {
            name: "ChaCha20-Poly1305".to_owned(),
            hash_name: None,
            length_bits: None,
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
        extractable,
        usages,
        key_bytes,
    })
}

pub(crate) fn import_chacha20_poly1305_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key_data: ImportKeyData,
    format: &str,
    extractable: bool,
    usages: &[String],
    promise: &PendingCryptoPromise<'s>,
) {
    match import_chacha20_poly1305_key_payload(key_data, format, extractable, usages.to_vec()) {
        Ok(payload) => resolve_crypto_key_payload(scope, promise, payload),
        Err(rejection) => promise.reject_webcrypto(scope, rejection),
    }
}

pub(crate) fn import_kdf_key_payload(
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    extractable: bool,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let key_bytes = match format {
        "raw" | "raw-secret" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "secret", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                if extractable {
                    return Err(WebCryptoRejection::Syntax);
                }
                bytes
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "jwk" | "spki" | "pkcs8" => return Err(WebCryptoRejection::NotSupported),
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    Ok(CryptoKeyClonePayload {
        key_type: "secret".to_owned(),
        algorithm: CryptoKeyAlgorithmClonePayload {
            name: webcrypto_algorithm_display_name(algorithm_name).to_owned(),
            hash_name: None,
            length_bits: None,
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
        extractable: false,
        usages,
        key_bytes,
    })
}

pub(crate) fn import_hmac_key_payload(
    key_data: ImportKeyData,
    format: &str,
    params: HmacImportParams,
    extractable: bool,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let key_bytes = match format {
        "raw" | "raw-secret" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(WebCryptoKeyAlgorithm::Hmac, "secret", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                bytes
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => {
                if !key_usages_are_valid(WebCryptoKeyAlgorithm::Hmac, "secret", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                import_hmac_jwk(*value, params.hash, extractable, &usages)?
            }
            ImportKeyData::Bytes(_) => return Err(WebCryptoRejection::Type),
        },
        "spki" | "pkcs8" => return Err(WebCryptoRejection::NotSupported),
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    let (key_bytes, length_bits) = validate_hmac_import_key_bytes(key_bytes, params.length_bits)
        .map_err(WebCryptoRejection::from)?;
    Ok(CryptoKeyClonePayload {
        key_type: "secret".to_owned(),
        algorithm: CryptoKeyAlgorithmClonePayload {
            name: "HMAC".to_owned(),
            hash_name: Some(params.hash.as_ref().to_ascii_uppercase()),
            length_bits: Some(length_bits),
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
        extractable,
        usages,
        key_bytes,
    })
}

pub(crate) fn import_x25519_key_payload(
    key_data: ImportKeyData,
    format: &str,
    extractable: bool,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let imported = match format {
        "raw" | "raw-public" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(WebCryptoKeyAlgorithm::X25519, "public", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                X25519ImportedKey::Public(
                    import_x25519_raw_public_key(&bytes).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "spki" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(WebCryptoKeyAlgorithm::X25519, "public", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                X25519ImportedKey::Public(
                    import_x25519_spki_public_key(&bytes).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "pkcs8" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(WebCryptoKeyAlgorithm::X25519, "private", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                X25519ImportedKey::Private(
                    import_x25519_pkcs8_private_key(&bytes).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => import_x25519_jwk(*value, extractable, &usages)?,
            ImportKeyData::Bytes(_) => return Err(WebCryptoRejection::Type),
        },
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    let (key_type, key_bytes) = match imported {
        X25519ImportedKey::Public(public_key) => ("public", public_key.to_vec()),
        X25519ImportedKey::Private(private_key) => ("private", private_key.as_ref().to_vec()),
    };
    if !key_usages_are_valid(WebCryptoKeyAlgorithm::X25519, key_type, &usages) {
        return Err(WebCryptoRejection::Syntax);
    }
    Ok(CryptoKeyClonePayload {
        key_type: key_type.to_owned(),
        algorithm: CryptoKeyAlgorithmClonePayload {
            name: "X25519".to_owned(),
            hash_name: None,
            length_bits: None,
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
        extractable,
        usages,
        key_bytes,
    })
}

pub(crate) fn import_rsa_key_payload(
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    hash: WebCryptoHashAlgorithm,
    extractable: bool,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let imported = match format {
        "spki" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "public", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                RsaImportedKey::Public(
                    import_rsa_spki_public_key(&bytes).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "pkcs8" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "private", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                RsaImportedKey::Private(
                    import_rsa_pkcs8_private_key(&bytes).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => {
                import_rsa_jwk(*value, algorithm_name, hash, extractable, &usages)?
            }
            ImportKeyData::Bytes(_) => return Err(WebCryptoRejection::Type),
        },
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    let (key_type, key_bytes, modulus_length_bits, public_exponent) = match imported {
        RsaImportedKey::Public(public_key) => (
            "public",
            public_key.key_bytes,
            public_key.modulus_length_bits,
            public_key.public_exponent,
        ),
        RsaImportedKey::Private(private_key) => (
            "private",
            private_key.key_bytes.as_slice().to_vec(),
            private_key.modulus_length_bits,
            private_key.public_exponent,
        ),
    };
    if !key_usages_are_valid(algorithm_name, key_type, &usages) {
        return Err(WebCryptoRejection::Syntax);
    }
    Ok(CryptoKeyClonePayload {
        key_type: key_type.to_owned(),
        algorithm: CryptoKeyAlgorithmClonePayload {
            name: webcrypto_algorithm_display_name(algorithm_name).to_owned(),
            hash_name: Some(hash.as_ref().to_ascii_uppercase()),
            length_bits: None,
            named_curve: None,
            modulus_length_bits: Some(modulus_length_bits),
            public_exponent: Some(public_exponent),
        },
        extractable,
        usages,
        key_bytes,
    })
}

pub(crate) fn import_ec_key_payload(
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    curve: WebCryptoEcNamedCurve,
    extractable: bool,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let imported = match format {
        "raw" | "raw-public" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "public", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                EcImportedKey::Public(
                    import_ec_raw_public_key(&bytes, curve).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "spki" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "public", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                EcImportedKey::Public(
                    import_ec_spki_public_key(&bytes, curve).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "pkcs8" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "private", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                EcImportedKey::Private(
                    import_ec_pkcs8_private_key(&bytes, curve).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => {
                import_ec_jwk(*value, algorithm_name, curve, extractable, &usages)?
            }
            ImportKeyData::Bytes(_) => return Err(WebCryptoRejection::Type),
        },
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    let (key_type, key_bytes, curve) = match imported {
        EcImportedKey::Public(public_key) => ("public", public_key.key_bytes, public_key.curve),
        EcImportedKey::Private(private_key) => (
            "private",
            private_key.key_bytes.as_slice().to_vec(),
            private_key.curve,
        ),
    };
    if !key_usages_are_valid(algorithm_name, key_type, &usages) {
        return Err(WebCryptoRejection::Syntax);
    }
    Ok(CryptoKeyClonePayload {
        key_type: key_type.to_owned(),
        algorithm: CryptoKeyAlgorithmClonePayload {
            name: webcrypto_algorithm_display_name(algorithm_name).to_owned(),
            hash_name: None,
            length_bits: None,
            named_curve: Some(curve.name().to_owned()),
            modulus_length_bits: None,
            public_exponent: None,
        },
        extractable,
        usages,
        key_bytes,
    })
}

pub(crate) fn import_okp_key_payload(
    key_data: ImportKeyData,
    format: &str,
    algorithm_name: WebCryptoKeyAlgorithm,
    curve: WebCryptoOkpCurve,
    extractable: bool,
    usages: Vec<String>,
) -> Result<CryptoKeyClonePayload, WebCryptoRejection> {
    let imported = match format {
        "raw" | "raw-public" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "public", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                OkpImportedKey::Public(
                    import_okp_raw_public_key(&bytes, curve).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "spki" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "public", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                OkpImportedKey::Public(
                    import_okp_spki_public_key(&bytes, curve).map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "pkcs8" => match key_data {
            ImportKeyData::Bytes(bytes) => {
                if !key_usages_are_valid(algorithm_name, "private", &usages) {
                    return Err(WebCryptoRejection::Syntax);
                }
                OkpImportedKey::Private(
                    import_okp_pkcs8_private_key(&bytes, curve)
                        .map_err(WebCryptoRejection::from)?,
                )
            }
            ImportKeyData::Jwk(_) => return Err(WebCryptoRejection::Type),
        },
        "jwk" => match key_data {
            ImportKeyData::Jwk(value) => import_okp_jwk(*value, curve, extractable, &usages)?,
            ImportKeyData::Bytes(_) => return Err(WebCryptoRejection::Type),
        },
        _ => return Err(WebCryptoRejection::NotSupported),
    };
    let (key_type, key_bytes) = match imported {
        OkpImportedKey::Public(public_key) => ("public", public_key.key_bytes),
        OkpImportedKey::Private(private_key) => {
            ("private", private_key.key_bytes.as_slice().to_vec())
        }
    };
    if !key_usages_are_valid(algorithm_name, key_type, &usages) {
        return Err(WebCryptoRejection::Syntax);
    }
    Ok(CryptoKeyClonePayload {
        key_type: key_type.to_owned(),
        algorithm: CryptoKeyAlgorithmClonePayload {
            name: webcrypto_algorithm_display_name(algorithm_name).to_owned(),
            hash_name: None,
            length_bits: None,
            named_curve: None,
            modulus_length_bits: None,
            public_exponent: None,
        },
        extractable,
        usages,
        key_bytes,
    })
}
