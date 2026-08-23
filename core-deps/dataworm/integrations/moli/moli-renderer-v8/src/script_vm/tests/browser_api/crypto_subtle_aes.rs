use super::*;

#[test]
fn crypto_subtle_aes_operation_params_match_chromium_failure_order() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesOperationParamFailureProbe = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const bytes = new Uint8Array([
              0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
              0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c
            ]);
            const data = new TextEncoder().encode("hello");
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const aesCbc = await subtle.importKey(
              "raw",
              bytes,
              { name: "AES-CBC" },
              false,
              ["encrypt", "decrypt"]
            );
            const aesGcm = await subtle.importKey(
              "raw",
              bytes,
              { name: "AES-GCM" },
              false,
              ["encrypt", "decrypt"]
            );

            // Chromium legacy tests:
            // crypto/subtle/aes-cbc/failures.html,
            // crypto/subtle/aes-ctr/failures.html, and
            // crypto/subtle/aes-gcm/failures.html. These still matter with
            // working AES backends because WebCrypto parameter normalization is
            // observable before the operation reaches key/backend execution.
            globalThis.__cryptoAesOperationParamFailureProbe = await Promise.all([
              rejectionName(subtle.encrypt({ name: "AES-CBC", iv: null }, aesCbc, data)),
              rejectionName(subtle.decrypt({ name: "AES-CBC" }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CBC", iv: 3 }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CBC", iv: new Uint8Array(0) }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CBC", iv: new ArrayBuffer(17) }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CTR", counter: null }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CTR" }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CTR", counter: new Uint8Array(0) }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CTR", counter: new Uint8Array(16), length: 256 }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CTR", counter: new Uint8Array(16), length: -3 }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CTR", counter: new Uint8Array(16), length: Infinity }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CTR", counter: new Uint8Array(0), length: 1 }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-CTR", counter: new Uint8Array(16), length: 0 }, aesCbc, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM" }, aesGcm, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: 3 }, aesGcm, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: "foo" }, aesGcm, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(16), additionalData: "5" }, aesGcm, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(16), additionalData: new Uint8Array(1), tagLength: "foo" }, aesGcm, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(16), additionalData: new Uint8Array(1), tagLength: -1 }, aesGcm, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(16), additionalData: new Uint8Array(1), tagLength: 8000 }, aesGcm, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(16), additionalData: new Uint8Array(1), tagLength: 0 }, aesGcm, data)),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(16), additionalData: new Uint8Array(1), tagLength: 130 }, aesGcm, data))
            ]);
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES operation parameter failure probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesOperationParamFailureProbe)")
        .expect("crypto subtle AES operation parameter failure promise chain should settle");

    assert_eq!(
        result,
        r#"["TypeError","TypeError","TypeError","OperationError","OperationError","TypeError","TypeError","TypeError","TypeError","TypeError","TypeError","OperationError","OperationError","TypeError","TypeError","TypeError","TypeError","TypeError","TypeError","TypeError","OperationError","OperationError"]"#
    );
}
#[test]
fn crypto_subtle_aes_product_resource_limits_reject_large_buffers() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesResourceLimitProbe = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const keyBytes = new Uint8Array([
              0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
              0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c
            ]);
            const aesGcm = await subtle.importKey(
              "raw",
              keyBytes,
              "AES-GCM",
              false,
              ["encrypt", "decrypt"]
            );
            const aesKw = await subtle.importKey(
              "raw",
              keyBytes,
              "AES-KW",
              false,
              ["unwrapKey"]
            );
            const tooLarge = new Uint8Array(16777217);
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );

            globalThis.__cryptoAesResourceLimitProbe = [
              await rejectionName(subtle.encrypt(
                { name: "AES-GCM", iv: new Uint8Array(12) },
                aesGcm,
                tooLarge
              )),
              await rejectionName(subtle.encrypt(
                { name: "AES-GCM", iv: tooLarge },
                aesGcm,
                new Uint8Array()
              )),
              await rejectionName(subtle.encrypt(
                { name: "AES-GCM", iv: new Uint8Array(12), additionalData: tooLarge },
                aesGcm,
                new Uint8Array()
              )),
              await rejectionName(subtle.unwrapKey(
                "raw",
                tooLarge,
                aesKw,
                "AES-KW",
                "AES-GCM",
                true,
                ["encrypt"]
              )),
              String(SubtleCrypto.supports(
                "encrypt",
                { name: "AES-GCM", iv: tooLarge }
              )),
              String(SubtleCrypto.supports(
                "encrypt",
                { name: "AES-GCM", iv: new Uint8Array(12), additionalData: tooLarge }
              ))
            ];
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES resource-limit probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesResourceLimitProbe)")
        .expect("crypto subtle AES resource-limit promise chain should settle");

    assert_eq!(
        result,
        r#"["OperationError","OperationError","OperationError","OperationError","false","false"]"#
    );
}
#[test]
fn crypto_subtle_aes_ctr_copies_counter_before_length_getter_side_effects() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesCtrCounterSnapshotFailures = ["pending"];
          (async () => {
            const failures = [];
            const subtle = crypto.subtle;
            const keyData = new Uint8Array([
              0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
              0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c
            ]);
            const data = new TextEncoder().encode("hello");
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const detach = (buffer) => {
              try {
                postMessage("", "*", [buffer]);
              } catch (error) {
                failures.push(`detach:${error.name}`);
              }
            };

            const key = await subtle.importKey(
              "raw",
              keyData,
              "AES-CTR",
              false,
              ["encrypt"]
            );
            const counter = new Uint8Array(16);
            const events = [];

            // Chromium legacy test:
            // crypto/subtle/neuter-algorithm-data-during-encrypt.html.
            // AES-CTR normalization copies `counter` before reading `length`,
            // so a length getter that transfers the original buffer must not
            // retroactively turn the operation into a short-counter error.
            const result = await rejectionName(subtle.encrypt({
              name: "AES-CTR",
              get counter() {
                events.push(`counter:${counter.byteLength}`);
                return counter;
              },
              get length() {
                events.push(`length-before:${counter.byteLength}`);
                detach(counter.buffer);
                events.push(`length-after:${counter.byteLength}`);
                return 8;
              }
            }, key, data));

            if (result !== "resolved") {
              failures.push(`result:${result}`);
            }
            if (events.join(",") !== "counter:16,length-before:16,length-after:0") {
              failures.push(`events:${events.join(",")}`);
            }

            globalThis.__cryptoAesCtrCounterSnapshotFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES-CTR counter snapshot probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesCtrCounterSnapshotFailures)")
        .expect("crypto subtle AES-CTR counter snapshot promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_aes_operation_accepts_detached_data_as_empty_buffer_source() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesDetachedDataFailures = ["pending"];
          (async () => {
            const failures = [];
            const subtle = crypto.subtle;
            const keyData = new Uint8Array([
              0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
              0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c
            ]);
            const iv = new Uint8Array(16);
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );

            const key = await subtle.importKey(
              "raw",
              keyData,
              "AES-CBC",
              false,
              ["encrypt"]
            );
            const data = new ArrayBuffer(1000);
            try {
              postMessage("", "*", [data]);
            } catch (error) {
              failures.push(`detach:${error.name}`);
            }
            if (data.byteLength !== 0) {
              failures.push(`byteLength:${data.byteLength}`);
            }

            // Chromium legacy test: crypto/subtle/encrypt-neutered-data.html.
            // A detached ArrayBuffer operation input is converted as an empty
            // BufferSource, so AES-CBC encrypts the PKCS#7 padding block
            // instead of failing BufferSource conversion.
            const result = await rejectionName(subtle.encrypt(
              { name: "AES-CBC", iv },
              key,
              data
            ));
            if (result !== "resolved") {
              failures.push(`result:${result}`);
            }

            // WPT WebCryptoAPI/encrypt_decrypt/aes.js transfers the operation
            // input from an algorithm.name getter. The later BufferSource
            // conversion observes the detached view as an empty input rather
            // than rejecting it.
            const duringCallData = new Uint8Array(1000);
            const duringCallResult = await subtle.encrypt(
              {
                get name() {
                  duringCallData.buffer.transfer();
                  return "AES-CBC";
                },
                iv
              },
              key,
              duringCallData
            ).then(
              (buffer) => buffer.byteLength,
              (error) => error.name
            );
            if (duringCallResult !== 16) {
              failures.push(`during-call-result:${duringCallResult}`);
            }

            globalThis.__cryptoAesDetachedDataFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES detached data probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesDetachedDataFailures)")
        .expect("crypto subtle AES detached data promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_aes_key_management_matches_wpt_symmetric_imports() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesKeyFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const sameBytes = (left, right) => {
              const a = new Uint8Array(left);
              const b = new Uint8Array(right);
              return a.length === b.length && a.every((value, index) => value === b[index]);
            };
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            // Chromium WPT: WebCryptoAPI/import_export/symmetric_importKey.js
            const rawCases = [
              { bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]), bits: 128, k: "AQIDBAUGBwgJCgsMDQ4PEA" },
              { bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32]), bits: 256, k: "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA" }
            ];
            const aes192 = {
              bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24]),
              k: "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcY"
            };
            const algorithms = [
              { name: "AES-CBC", suffix: "CBC", usages: ["encrypt", "decrypt"] },
              { name: "AES-CTR", suffix: "CTR", usages: ["encrypt", "decrypt"] },
              { name: "AES-GCM", suffix: "GCM", usages: ["encrypt", "decrypt"] },
              { name: "AES-KW", suffix: "KW", usages: ["wrapKey", "unwrapKey"] }
            ];
            const generatedKeyMaterial = new Set();

            const duplicateGenerated = await subtle.generateKey(
              { name: "AES-GCM", length: 128 },
              true,
              ["encrypt", "decrypt", "encrypt", "wrapKey"]
            );
            if (duplicateGenerated.usages.join(",") !== "encrypt,decrypt,wrapKey") {
              failures.push("duplicate-usages:generate");
            }
            const duplicateRaw = await subtle.importKey(
              "raw",
              rawCases[0].bytes,
              "AES-GCM",
              true,
              ["decrypt", "decrypt", "encrypt"]
            );
            const duplicateRawJwk = await subtle.exportKey("jwk", duplicateRaw);
            if (
              duplicateRaw.usages.join(",") !== "encrypt,decrypt" ||
              duplicateRawJwk.key_ops.join(",") !== "encrypt,decrypt"
            ) {
              failures.push("duplicate-usages:import-export");
            }
            // Chromium: crypto/subtle/jwk-import-use-values.html.  AES JWKs
            // accept `use: "enc"` but reject signing use and duplicate key_ops.
            const useConstrainedJwk = await subtle.importKey(
              "jwk",
              { kty: "oct", k: rawCases[0].k, alg: "A128GCM", use: "enc" },
              { name: "AES-GCM" },
              true,
              ["encrypt"]
            );
            if (useConstrainedJwk.usages.join(",") !== "encrypt") {
              failures.push("jwk-use:enc");
            }

            for (const algorithm of algorithms) {
              for (const keyCase of rawCases) {
                const rawKey = await subtle.importKey(
                  "raw",
                  keyCase.bytes,
                  { name: algorithm.name },
                  true,
                  algorithm.usages
                );
                if (
                  rawKey.type !== "secret" ||
                  rawKey.algorithm.name !== algorithm.name ||
                  rawKey.algorithm.length !== keyCase.bits ||
                  rawKey.usages.join(",") !== algorithm.usages.join(",") ||
                  !sameBytes(await subtle.exportKey("raw", rawKey), keyCase.bytes)
                ) {
                  failures.push(algorithm.name + ":raw:" + keyCase.bits);
                }

                const jwk = {
                  kty: "oct",
                  k: keyCase.k,
                  alg: "A" + keyCase.bits + algorithm.suffix
                };
                const jwkKey = await subtle.importKey(
                  "jwk",
                  jwk,
                  { name: algorithm.name },
                  true,
                  algorithm.usages
                );
                const exported = await subtle.exportKey("jwk", jwkKey);
                if (
                  exported.kty !== "oct" ||
                  exported.k !== jwk.k ||
                  exported.alg !== jwk.alg ||
                  exported.key_ops.join(",") !== algorithm.usages.join(",")
                ) {
                  failures.push(algorithm.name + ":jwk:" + keyCase.bits);
                }
              }

              // Chromium WPT: WebCryptoAPI/generateKey/successes.js.
              for (const length of [128, 192, 256]) {
                for (const extractable of [true, false]) {
                  const generated = await subtle.generateKey(
                    { name: algorithm.name, length },
                    extractable,
                    algorithm.usages
                  );
                  if (
                    generated.type !== "secret" ||
                    generated.extractable !== extractable ||
                    generated.algorithm.name !== algorithm.name ||
                    generated.algorithm.length !== length ||
                    generated.usages.join(",") !== algorithm.usages.join(",")
                  ) {
                    failures.push(`${algorithm.name}:generate-shape:${length}:${extractable}`);
                  }
                  if (extractable) {
                    const raw = new Uint8Array(await subtle.exportKey("raw", generated));
                    const rawKey = Array.from(raw).join(",");
                    if (raw.byteLength !== length / 8) {
                      failures.push(`${algorithm.name}:generate-length:${length}:${raw.byteLength}`);
                    }
                    if (generatedKeyMaterial.has(rawKey)) {
                      failures.push(`${algorithm.name}:generate-duplicate:${length}`);
                    }
                    generatedKeyMaterial.add(rawKey);
                  }
                }
              }
            }

            // Chromium legacy test: crypto/subtle/aes-generateKey.html. This
            // keeps the crawler-facing AES key-management surface honest even
            // while encrypt/decrypt are deliberately unsupported.
            for (const algorithmName of ["AES-CBC", "AES-GCM"]) {
              for (const usages of [["encrypt"], ["decrypt", "wrapKey"], ["encrypt", "wrapKey", "unwrapKey"]]) {
                for (const length of [128, 192, 256]) {
                  for (const extractable of [true, false]) {
                    const generated = await subtle.generateKey(
                      { name: algorithmName.toLowerCase(), length },
                      extractable,
                      usages
                    );
                    if (
                      generated.extractable !== extractable ||
                      generated.algorithm.name !== algorithmName ||
                      generated.algorithm.length !== length ||
                      generated.usages.join(",") !== usages.join(",")
                    ) {
                      failures.push(`${algorithmName}:legacy-generate:${length}:${extractable}:${usages.join("+")}`);
                    }
                    if (extractable) {
                      const raw = new Uint8Array(await subtle.exportKey("raw", generated));
                      if (raw.byteLength !== length / 8) {
                        failures.push(`${algorithmName}:legacy-generate-length:${length}:${raw.byteLength}`);
                      }
                    }
                  }
                }
              }
            }

            for (const method of ["encrypt", "decrypt", "wrapKey", "unwrapKey"]) {
              if (typeof subtle[method] !== "function") {
                failures.push("missing-method:" + method);
              }
            }
            const aesGcmKey = await subtle.generateKey(
              { name: "AES-GCM", length: 128 },
              true,
              ["encrypt", "decrypt"]
            );
            const aesKwKey = await subtle.generateKey(
              { name: "AES-KW", length: 128 },
              true,
              ["wrapKey", "unwrapKey"]
            );
            const errors = await Promise.all([
              rejectionName(subtle.importKey("raw", new Uint8Array([1, 2, 3]), "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", { kty: "oct", k: rawCases[0].k, alg: "A128CBC" }, "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.importKey("raw", rawCases[0].bytes, "AES-GCM", true, [])),
              rejectionName(subtle.importKey("raw", rawCases[0].bytes, "AES-KW", true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", { kty: "oct", k: rawCases[0].k, alg: "A128GCM", use: "sig" }, "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", { kty: "oct", k: rawCases[0].k, alg: "A128GCM", key_ops: ["encrypt", "encrypt"] }, "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", { kty: "oct", k: rawCases[0].k, alg: "A128GCM", use: "enc", key_ops: ["encrypt", "sign"] }, "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", { kty: "oct", k: rawCases[0].k, alg: "A128GCM", key_ops: ["decrypt"] }, "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.generateKey({ name: "AES-GCM", length: 64 }, true, ["encrypt"])),
              rejectionName(subtle.generateKey({ name: "AES-GCM", length: 192 }, true, ["encrypt"])),
              rejectionName(subtle.importKey("raw", aes192.bytes, "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", { kty: "oct", k: aes192.k, alg: "A192GCM" }, "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", { kty: "oct", k: aes192.k, alg: "A128GCM" }, "AES-GCM", true, ["encrypt"])),
              rejectionName(subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(12) }, aesGcmKey, new Uint8Array())),
              rejectionName(subtle.decrypt({ name: "AES-GCM", iv: new Uint8Array(12) }, aesGcmKey, new Uint8Array())),
              rejectionName(subtle.wrapKey("raw", aesGcmKey, aesKwKey, { name: "AES-KW" })),
              rejectionName(subtle.unwrapKey("raw", new Uint8Array(), aesKwKey, { name: "AES-KW" }, "AES-GCM", true, ["encrypt"]))
            ]);
            const expected = [
              "DataError",
              "DataError",
              "SyntaxError",
              "SyntaxError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "OperationError",
              "resolved",
              "resolved",
              "resolved",
              "DataError",
              "resolved",
              "OperationError",
              "resolved",
              "OperationError"
            ];
            if (errors.join(",") !== expected.join(",")) {
              failures.push("errors:" + errors.join(","));
            }
            globalThis.__cryptoAesKeyFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES key probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesKeyFailures)")
        .expect("crypto subtle AES key promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_aes_import_export_matches_wpt_symmetric_matrix() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesSymmetricImportMatrixFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const allNonemptySubsets = (values) => {
              const results = [];
              for (let i = 0; i < values.length; i++) {
                const first = values[i];
                const remaining = values.slice(i + 1);
                results.push([first]);
                for (const subset of allNonemptySubsets(remaining)) {
                  subset.push(first);
                  results.push(subset);
                }
              }
              return results;
            };
            const validUsages = (usages) => {
              const results = allNonemptySubsets(usages);
              results.push(usages.concat(usages));
              return results;
            };
            const sameBytes = (left, right) => {
              const a = new Uint8Array(left);
              const b = new Uint8Array(right);
              return a.length === b.length && a.every((value, index) => value === b[index]);
            };
            const sameUsageSet = (actual, expected) => {
              const deduped = [...new Set(expected)];
              return actual.length === deduped.length &&
                deduped.every((usage) => actual.includes(usage));
            };
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const rawCases = [
              { bits: 128, bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]), k: "AQIDBAUGBwgJCgsMDQ4PEA" },
              { bits: 192, bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24]), k: "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcY" },
              { bits: 256, bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32]), k: "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA" }
            ];
            const algorithms = [
              { name: "AES-CBC", suffix: "CBC", usages: ["encrypt", "decrypt"] },
              { name: "AES-CTR", suffix: "CTR", usages: ["encrypt", "decrypt"] },
              { name: "AES-GCM", suffix: "GCM", usages: ["encrypt", "decrypt"] },
              { name: "AES-KW", suffix: "KW", usages: ["wrapKey", "unwrapKey"] }
            ];

            // Chromium WPT: WebCryptoAPI/import_export/symmetric_importKey.js.
            for (const algorithm of algorithms) {
              for (const keyCase of rawCases) {
                for (const format of ["raw", "jwk"]) {
                  const keyData = format === "raw"
                    ? keyCase.bytes
                    : { kty: "oct", k: keyCase.k, alg: "A" + keyCase.bits + algorithm.suffix };
                  for (const extractable of [true, false]) {
                    for (const usages of validUsages(algorithm.usages)) {
                      const key = await subtle.importKey(
                        format,
                        keyData,
                        { name: algorithm.name },
                        extractable,
                        usages
                      );
                      if (
                        key.constructor !== CryptoKey ||
                        key.type !== "secret" ||
                        key.extractable !== extractable ||
                        key.algorithm.name !== algorithm.name ||
                        key.algorithm.length !== keyCase.bits ||
                        !sameUsageSet(key.usages, usages) ||
                        key[Symbol.toStringTag] !== "CryptoKey"
                      ) {
                        failures.push([
                          "shape",
                          format,
                          algorithm.name,
                          keyCase.bits,
                          extractable,
                          usages.join("+"),
                          key.type,
                          key.algorithm.name,
                          key.algorithm.length,
                          key.usages.join("+")
                        ].join(":"));
                      }
                      if (extractable) {
                        const exportedRaw = new Uint8Array(await subtle.exportKey("raw", key));
                        const exportedJwk = await subtle.exportKey("jwk", key);
                        if (
                          !sameBytes(exportedRaw, keyCase.bytes) ||
                          exportedJwk.kty !== "oct" ||
                          exportedJwk.k !== keyCase.k ||
                          exportedJwk.alg !== "A" + keyCase.bits + algorithm.suffix ||
                          exportedJwk.ext !== true ||
                          !sameUsageSet(exportedJwk.key_ops, usages)
                        ) {
                          failures.push([
                            "export",
                            format,
                            algorithm.name,
                            keyCase.bits,
                            exportedRaw.byteLength,
                            exportedJwk.kty,
                            exportedJwk.alg,
                            exportedJwk.key_ops.join("+")
                          ].join(":"));
                        }
                      } else {
                        const rawError = await rejectionName(subtle.exportKey("raw", key));
                        const jwkError = await rejectionName(subtle.exportKey("jwk", key));
                        if (rawError !== "InvalidAccessError" || jwkError !== "InvalidAccessError") {
                          failures.push("unextractable:" + format + ":" + algorithm.name + ":" + rawError + ":" + jwkError);
                        }
                      }
                    }

                    const emptyError = await rejectionName(subtle.importKey(
                      format,
                      keyData,
                      { name: algorithm.name },
                      extractable,
                      []
                    ));
                    if (emptyError !== "SyntaxError") {
                      failures.push("empty-usages:" + format + ":" + algorithm.name + ":" + keyCase.bits + ":" + extractable + ":" + emptyError);
                    }
                  }
                }
              }
            }

            const aes192Case = rawCases.find((entry) => entry.bits === 192);
            for (const algorithm of algorithms) {
              const invalidUsage = algorithm.name === "AES-KW" ? ["encrypt"] : ["sign"];
              for (const format of ["raw", "jwk"]) {
                const keyData = format === "raw"
                  ? aes192Case.bytes
                  : { kty: "oct", k: aes192Case.k, alg: "A192" + algorithm.suffix };
                const error = await rejectionName(subtle.importKey(
                  format,
                  keyData,
                  { name: algorithm.name },
                  true,
                  invalidUsage
                ));
                if (error !== "SyntaxError") {
                  failures.push("aes-192-invalid-usage:" + format + ":" + algorithm.name + ":" + error);
                }
              }
            }

            globalThis.__cryptoAesSymmetricImportMatrixFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES symmetric import matrix should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesSymmetricImportMatrixFailures)")
        .expect("crypto subtle AES symmetric import matrix promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_aes_generate_key_matches_wpt_success_matrix() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesGenerateKeyWptMatrixFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const allNonemptySubsets = (values) => {
              const results = [];
              for (let i = 0; i < values.length; i++) {
                const first = values[i];
                const remaining = values.slice(i + 1);
                results.push([first]);
                for (const subset of allNonemptySubsets(remaining)) {
                  subset.push(first);
                  results.push(subset);
                }
              }
              return results;
            };
            const validUsages = (usages) => {
              const results = allNonemptySubsets(usages);
              results.push(usages.concat(usages));
              return results;
            };
            const nameVariants = (name) => {
              const upper = name.toUpperCase();
              const lower = name.toLowerCase();
              return [...new Set([upper, lower, upper.slice(0, 1) + lower.slice(1)])];
            };
            const sameUsageSet = (actual, expected) => {
              const deduped = [...new Set(expected)];
              return actual.length === deduped.length &&
                deduped.every((usage) => actual.includes(usage));
            };
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const algorithms = [
              { name: "AES-CBC", suffix: "CBC", usages: ["encrypt", "decrypt", "wrapKey", "unwrapKey"] },
              { name: "AES-CTR", suffix: "CTR", usages: ["encrypt", "decrypt", "wrapKey", "unwrapKey"] },
              { name: "AES-GCM", suffix: "GCM", usages: ["encrypt", "decrypt", "wrapKey", "unwrapKey"] },
              { name: "AES-KW", suffix: "KW", usages: ["wrapKey", "unwrapKey"] }
            ];

            // Chromium WPT: WebCryptoAPI/generateKey/successes_AES-*.
            for (const algorithm of algorithms) {
              for (const name of nameVariants(algorithm.name)) {
                for (const length of [128, 192, 256]) {
                  for (const usages of validUsages(algorithm.usages)) {
                    for (const extractable of [false, true]) {
                      const key = await subtle.generateKey(
                        { name, length },
                        extractable,
                        usages
                      );
                      if (
                        key.constructor !== CryptoKey ||
                        key.type !== "secret" ||
                        key.extractable !== extractable ||
                        key.algorithm.name !== algorithm.name ||
                        key.algorithm.length !== length ||
                        !sameUsageSet(key.usages, usages) ||
                        key[Symbol.toStringTag] !== "CryptoKey"
                      ) {
                        failures.push([
                          "shape",
                          algorithm.name,
                          name,
                          length,
                          extractable,
                          usages.join("+"),
                          key.type,
                          key.algorithm.name,
                          key.algorithm.length,
                          key.usages.join("+")
                        ].join(":"));
                      }
                      if (extractable) {
                        const raw = new Uint8Array(await subtle.exportKey("raw", key));
                        const jwk = await subtle.exportKey("jwk", key);
                        if (
                          raw.byteLength !== length / 8 ||
                          jwk.kty !== "oct" ||
                          jwk.alg !== "A" + length + algorithm.suffix ||
                          jwk.ext !== true ||
                          !sameUsageSet(jwk.key_ops, usages)
                        ) {
                          failures.push([
                            "export",
                            algorithm.name,
                            name,
                            length,
                            raw.byteLength,
                            jwk.kty,
                            jwk.alg,
                            jwk.key_ops.join("+")
                          ].join(":"));
                        }
                      }
                    }
                  }
                }
              }
            }

            globalThis.__cryptoAesGenerateKeyWptMatrixFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES generateKey WPT matrix should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesGenerateKeyWptMatrixFailures)")
        .expect("crypto subtle AES generateKey WPT matrix promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_aes_generate_key_matches_wpt_failure_matrix() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesGenerateKeyWptFailureMatrixFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const allNonemptySubsets = (values) => {
              const results = [];
              for (let i = 0; i < values.length; i++) {
                const first = values[i];
                const remaining = values.slice(i + 1);
                results.push([first]);
                for (const subset of allNonemptySubsets(remaining)) {
                  subset.push(first);
                  results.push(subset);
                }
              }
              return results;
            };
            const validUsages = (usages, emptyIsValid) => {
              const results = allNonemptySubsets(usages);
              if (emptyIsValid && usages.length !== 0) {
                results.push([]);
              }
              results.push(usages.concat(usages));
              return results;
            };
            const invalidUsages = (usages) => {
              const illegal = ["encrypt", "decrypt", "sign", "verify", "wrapKey", "unwrapKey", "deriveKey", "deriveBits"]
                .filter((usage) => !usages.includes(usage));
              const good = validUsages(usages, false);
              const results = [];
              for (const illegalUsage of illegal) {
                results.push([illegalUsage]);
                for (const usageCombination of good) {
                  results.push(usageCombination.concat([illegalUsage]));
                }
              }
              return results;
            };
            const algorithms = [
              { name: "AES-CBC", usages: ["encrypt", "decrypt", "wrapKey", "unwrapKey"] },
              { name: "AES-CTR", usages: ["encrypt", "decrypt", "wrapKey", "unwrapKey"] },
              { name: "AES-GCM", usages: ["encrypt", "decrypt", "wrapKey", "unwrapKey"] },
              { name: "AES-KW", usages: ["wrapKey", "unwrapKey"] }
            ];
            const badAlgorithmNames = [
              "AES",
              { name: "AES" },
              { name: "AES", length: 128 },
              { name: "AES-CMAC", length: 128 },
              { name: "AES-CFB", length: 128 }
            ];
            const badAlgorithmUsageCases = validUsages(["decrypt", "sign", "deriveBits"], true);

            // Chromium WPT: WebCryptoAPI/generateKey/failures_AES-*.
            for (const algorithm of badAlgorithmNames) {
              for (const usages of badAlgorithmUsageCases) {
                for (const extractable of [false, true, "RED", 7]) {
                  const error = await rejectionName(subtle.generateKey(algorithm, extractable, usages));
                  if (error !== "NotSupportedError") {
                    failures.push("bad-algorithm:" + JSON.stringify(algorithm) + ":" + usages.join("+") + ":" + extractable + ":" + error);
                  }
                }
              }
            }

            for (const usages of badAlgorithmUsageCases) {
              for (const extractable of [false, true, "RED", 7]) {
                const error = await rejectionName(subtle.generateKey({}, extractable, usages));
                if (error !== "TypeError") {
                  failures.push("empty-algorithm:" + usages.join("+") + ":" + extractable + ":" + error);
                }
              }
            }

            for (const algorithm of algorithms) {
              for (const length of [128, 192, 256]) {
                for (const usages of invalidUsages(algorithm.usages)) {
                  const error = await rejectionName(subtle.generateKey(
                    { name: algorithm.name, length },
                    true,
                    usages
                  ));
                  if (error !== "SyntaxError") {
                    failures.push("bad-usages:" + algorithm.name + ":" + length + ":" + usages.join("+") + ":" + error);
                  }
                }
              }
            }

            for (const algorithm of algorithms) {
              for (const length of [64, 127, 129, 255, 257, 512]) {
                for (const usages of validUsages(algorithm.usages, true)) {
                  for (const extractable of [false, true]) {
                    const error = await rejectionName(subtle.generateKey(
                      { name: algorithm.name, length },
                      extractable,
                      usages
                    ));
                    if (error !== "OperationError") {
                      failures.push("bad-length:" + algorithm.name + ":" + length + ":" + usages.join("+") + ":" + extractable + ":" + error);
                    }
                  }
                }
              }
            }

            for (const algorithm of algorithms) {
              for (const length of [128, 192, 256]) {
                for (const extractable of [false, true]) {
                  const error = await rejectionName(subtle.generateKey(
                    { name: algorithm.name, length },
                    extractable,
                    []
                  ));
                  if (error !== "SyntaxError") {
                    failures.push("empty-usages:" + algorithm.name + ":" + length + ":" + extractable + ":" + error);
                  }
                }
              }
            }

            globalThis.__cryptoAesGenerateKeyWptFailureMatrixFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES generateKey WPT failure matrix should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesGenerateKeyWptFailureMatrixFailures)")
        .expect("crypto subtle AES generateKey WPT failure matrix promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_aes_kw_key_manipulation_matches_chromium_legacy() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesKwKeyManipulationFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const sameBytes = (left, right) => {
              const a = new Uint8Array(left);
              const b = new Uint8Array(right);
              return a.length === b.length &&
                a.every((value, index) => value === b[index]);
            };

            // Chromium legacy tests:
            // crypto/subtle/aes-kw/key-manipulation.html and
            // crypto/subtle/aes-kw/generateKey-failures.html. AES-KW real
            // wrapping is still unsupported locally, but key creation,
            // export/import, and wrong-operation algorithm rejection are
            // observable key-management behavior that should match Chromium.
            const key = await subtle.generateKey(
              { name: "aes-kw", length: 256 },
              true,
              ["wrapKey", "unwrapKey"]
            );
            if (
              key.toString() !== "[object CryptoKey]" ||
              key.type !== "secret" ||
              key.extractable !== true ||
              key.algorithm.name !== "AES-KW" ||
              key.algorithm.length !== 256 ||
              key.usages.join(",") !== "wrapKey,unwrapKey"
            ) {
              failures.push([
                "generated-shape",
                key.toString(),
                key.type,
                key.extractable,
                key.algorithm.name,
                key.algorithm.length,
                key.usages.join("+")
              ].join(":"));
            }

            const wrongAlgorithm = await rejectionName(subtle.wrapKey(
              "raw",
              key,
              key,
              { name: "AES-CBC", iv: new Uint8Array(16) }
            ));
            if (wrongAlgorithm !== "InvalidAccessError") {
              failures.push("wrong-algorithm:" + wrongAlgorithm);
            }

            const exported = await subtle.exportKey("raw", key);
            if (
              exported.toString() !== "[object ArrayBuffer]" ||
              exported.byteLength !== 32
            ) {
              failures.push("exported-shape:" + exported.toString() + ":" + exported.byteLength);
            }

            const imported = await subtle.importKey(
              "raw",
              exported,
              { name: "aes-kw" },
              true,
              ["wrapKey", "unwrapKey"]
            );
            if (
              imported.toString() !== "[object CryptoKey]" ||
              imported.type !== "secret" ||
              imported.extractable !== true ||
              imported.algorithm.name !== "AES-KW" ||
              imported.algorithm.length !== 256 ||
              imported.usages.join(",") !== "wrapKey,unwrapKey" ||
              !sameBytes(await subtle.exportKey("raw", imported), exported)
            ) {
              failures.push([
                "imported-shape",
                imported.toString(),
                imported.type,
                imported.extractable,
                imported.algorithm.name,
                imported.algorithm.length,
                imported.usages.join("+")
              ].join(":"));
            }

            const generateFailures = await Promise.all([
              rejectionName(subtle.generateKey({ name: "aes-kw" }, true, ["wrapKey", "unwrapKey"])),
              rejectionName(subtle.generateKey({ name: "aes-kw", length: 70000 }, true, ["wrapKey", "unwrapKey"])),
              rejectionName(subtle.generateKey({ name: "aes-kw", length: -3 }, true, ["wrapKey", "unwrapKey"])),
              rejectionName(subtle.generateKey({ name: "aes-kw", length: -Infinity }, true, ["wrapKey", "unwrapKey"]))
            ]);
            if (generateFailures.join(",") !== "TypeError,TypeError,TypeError,TypeError") {
              failures.push("generate-failures:" + generateFailures.join(","));
            }

            globalThis.__cryptoAesKwKeyManipulationFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES-KW legacy key manipulation probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesKwKeyManipulationFailures)")
        .expect("crypto subtle AES-KW legacy key manipulation promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_aes_legacy_access_boundaries_match_chromium() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoAesLegacyBoundaryFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );

            // Chromium legacy test:
            // crypto/subtle/import-aes-key-bad-length.html. AES raw imports
            // reject off-by-one lengths with DataError.
            const invalidLengths = [0, 1, 15, 17, 23, 25, 31, 33, 64];
            const algorithms = ["AES-CBC", "AES-CTR", "AES-GCM", "AES-KW"];
            for (const algorithm of algorithms) {
              const usages = algorithm === "AES-KW"
                ? ["wrapKey"]
                : ["encrypt"];
              for (const byteLength of invalidLengths) {
                const error = await rejectionName(subtle.importKey(
                  "raw",
                  new Uint8Array(byteLength),
                  algorithm,
                  false,
                  usages
                ));
                if (error !== "DataError") {
                  failures.push(`${algorithm}:raw-length:${byteLength}:${error}`);
                }
              }
              const supported192 = await rejectionName(subtle.importKey(
                "raw",
                new Uint8Array(24),
                algorithm,
                false,
                usages
              ));
              if (supported192 !== "resolved") {
                failures.push(`${algorithm}:raw-length:24:${supported192}`);
              }
            }

            const keyData = new Uint8Array(16);
            const hmacExtractable = await subtle.importKey(
              "raw",
              keyData,
              { name: "HMAC", hash: "SHA-1" },
              true,
              ["sign"]
            );
            const hmacUnextractable = await subtle.importKey(
              "raw",
              keyData,
              { name: "HMAC", hash: "SHA-1" },
              false,
              ["sign"]
            );
            const aesGcmEncrypt = await subtle.importKey(
              "raw",
              keyData,
              "AES-GCM",
              false,
              ["encrypt"]
            );
            const aesCbcEncryptOnly = await subtle.importKey(
              "raw",
              keyData,
              "AES-CBC",
              true,
              ["encrypt"]
            );
            const aesCbcDecryptOnly = await subtle.importKey(
              "raw",
              keyData,
              "AES-CBC",
              true,
              ["decrypt"]
            );
            const aesCbcWrapOnly = await subtle.importKey(
              "raw",
              keyData,
              "AES-CBC",
              true,
              ["wrapKey"]
            );
            const aesKwWrapOnly = await subtle.importKey(
              "raw",
              keyData,
              "AES-KW",
              true,
              ["wrapKey"]
            );

            const errors = await Promise.all([
              // Chromium legacy test:
              // crypto/subtle/aes-key-algorithm-mismatch.html.
              rejectionName(subtle.encrypt(
                { name: "AES-CBC", iv: new Uint8Array(16) },
                aesGcmEncrypt,
                new Uint8Array([1, 2, 3])
              )),
              // Chromium legacy test:
              // crypto/subtle/wrapKey-lacks-usage.html.
              rejectionName(subtle.wrapKey(
                "raw",
                hmacExtractable,
                aesCbcEncryptOnly,
                { name: "AES-CBC", iv: new Uint8Array(16) }
              )),
              // Chromium legacy test:
              // crypto/subtle/unwrapKey-lacks-usage.html.
              rejectionName(subtle.unwrapKey(
                "raw",
                new Uint8Array(16),
                aesCbcDecryptOnly,
                { name: "AES-CBC", iv: new Uint8Array(16) },
                "AES-CBC",
                true,
                ["encrypt"]
              )),
              // Chromium legacy test:
              // crypto/subtle/wrapKey-unextractable.html.
              rejectionName(subtle.wrapKey(
                "raw",
                hmacUnextractable,
                aesCbcWrapOnly,
                { name: "AES-CBC", iv: new Uint8Array(16) }
              )),
              rejectionName(subtle.unwrapKey(
                "raw",
                new Uint8Array(16),
                aesKwWrapOnly,
                "AES-KW",
                "AES-CBC",
                true,
                ["encrypt"]
              ))
            ]);
            const expected = [
              "InvalidAccessError",
              "InvalidAccessError",
              "InvalidAccessError",
              "InvalidAccessError",
              "InvalidAccessError"
            ];
            if (errors.join(",") !== expected.join(",")) {
              failures.push("access-errors:" + errors.join(","));
            }

            globalThis.__cryptoAesLegacyBoundaryFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle AES legacy boundary probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoAesLegacyBoundaryFailures)")
        .expect("crypto subtle AES legacy boundary promise chain should settle");

    assert_eq!(result, "[]");
}
