use super::*;

#[test]
fn crypto_subtle_hmac_generate_key_matches_chromium_legacy() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoHmacGenerateKeyProbe = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const keySummary = (key) => [
              key.type,
              String(key.extractable),
              key.algorithm.name,
              String(key.algorithm.length),
              key.usages.join(",")
            ].join(":");

            // Chromium legacy web test:
            // crypto/subtle/hmac/generate-key.html.
            const results = [
              await rejectionName(subtle.generateKey(
                "hmac",
                true,
                ["sign", "verify"]
              )),
              await rejectionName(subtle.generateKey(
                { name: "hmac" },
                true,
                ["sign", "verify"]
              )),
              await rejectionName(subtle.generateKey(
                { name: "hmac", length: undefined, hash: { name: "sha-1" } },
                true,
                ["sign", "verify"]
              )),
              await rejectionName(subtle.generateKey(
                { name: "hmac", length: {}, hash: { name: "sha-1" } },
                true,
                ["sign", "verify"]
              ))
            ];
            results.push(keySummary(await subtle.generateKey(
              { name: "hmac", hash: { name: "sha-1" } },
              true,
              ["sign", "verify"]
            )));
            results.push(keySummary(await subtle.generateKey(
              { name: "hmac", hash: { name: "sha-1" }, length: 40 },
              true,
              ["sign"]
            )));
            const oneBitKey = await subtle.generateKey(
              { name: "hmac", hash: { name: "sha-1" }, length: 1 },
              true,
              ["sign"]
            );
            const oneBitRaw = new Uint8Array(await subtle.exportKey("raw", oneBitKey));
            results.push([
              "one",
              oneBitKey.algorithm.length,
              oneBitRaw.byteLength,
              oneBitRaw[0] & 0x7f
            ].join(":"));
            const oddLengthKey = await subtle.generateKey(
              { name: "hmac", hash: { name: "sha-1" }, length: 12 },
              true,
              ["sign"]
            );
            const oddLengthRaw = new Uint8Array(await subtle.exportKey("raw", oddLengthKey));
            results.push([
              "odd",
              oddLengthKey.algorithm.length,
              oddLengthRaw.byteLength,
              oddLengthRaw[1] & 0x0f
            ].join(":"));

            // Chromium legacy web test:
            // crypto/subtle/hmac/generateKey-failures.html.
            results.push((await Promise.all([
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "sha-256" }, length: -3 },
                true,
                ["sign", "verify"]
              )),
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "" }, length: 48 },
                true,
                ["sign", "verify"]
              )),
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "sha-256" }, length: 65537 },
                true,
                ["sign", "verify"]
              )),
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "sha-256" }, length: 5000000000 },
                true,
                ["sign", "verify"]
              )),
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "sha-256" }, length: NaN },
                true,
                ["sign", "verify"]
              )),
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "sha-256" }, length: Infinity },
                true,
                ["sign", "verify"]
              )),
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "sha-256" }, length: -Infinity },
                true,
                ["sign", "verify"]
              )),
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "sha-256" }, length: crypto },
                true,
                ["sign", "verify"]
              )),
              rejectionName(subtle.generateKey(
                { name: "hmac", hash: { name: "sha-256" }, length: undefined },
                true,
                ["sign", "verify"]
              ))
            ])).join(","));

            globalThis.__cryptoHmacGenerateKeyProbe = results;
          })();
        })()
        "#,
    )
    .expect("crypto subtle HMAC generateKey Chromium legacy probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoHmacGenerateKeyProbe)")
        .expect("crypto subtle HMAC generateKey promise chain should settle");

    assert_eq!(
        result,
        r#"["TypeError","TypeError","TypeError","TypeError","secret:true:HMAC:512:sign,verify","secret:true:HMAC:40:sign","one:1:1:0","odd:12:2:0","TypeError,NotSupportedError,OperationError,TypeError,TypeError,TypeError,TypeError,TypeError,TypeError"]"#
    );
}
#[test]
fn crypto_subtle_hmac_generate_key_matches_wpt_success_matrix() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoHmacGenerateKeyWptMatrixFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const nameVariants = ["HMAC", "hmac", "Hmac"];
            const algorithmSpecs = [
              { hash: "SHA-1", length: 160, expectedLength: 160 },
              { hash: "SHA-256", length: 256, expectedLength: 256 },
              { hash: "SHA-384", length: 384, expectedLength: 384 },
              { hash: "SHA-512", length: 512, expectedLength: 512 },
              { hash: "SHA-1", expectedLength: 512 },
              { hash: "SHA-256", expectedLength: 512 },
              { hash: "SHA-384", expectedLength: 1024 },
              { hash: "SHA-512", expectedLength: 1024 }
            ];
            const usageCases = [
              ["sign"],
              ["verify"],
              ["verify", "sign"],
              ["sign", "verify", "sign", "verify"]
            ];
            const normalizedUsages = (usages) => ["sign", "verify"]
              .filter((usage) => usages.includes(usage));

            // Chromium WPT: WebCryptoAPI/generateKey/successes_HMAC.
            // The upstream helper expands HMAC across case variants, all
            // supported SHA families, explicit/default key lengths, usage
            // subsets, repeated usages, and extractable true/false.
            for (const name of nameVariants) {
              for (const spec of algorithmSpecs) {
                for (const usages of usageCases) {
                  for (const extractable of [false, true]) {
                    const label = [
                      name,
                      spec.hash,
                      spec.length === undefined ? "default" : spec.length,
                      usages.join("+"),
                      extractable
                    ].join(":");
                    const algorithm = { name, hash: spec.hash };
                    if (spec.length !== undefined) {
                      algorithm.length = spec.length;
                    }
                    const key = await subtle.generateKey(algorithm, extractable, usages);
                    const expectedUsages = normalizedUsages(usages);
                    if (
                      !(key instanceof CryptoKey) ||
                      key.constructor !== CryptoKey ||
                      key.type !== "secret" ||
                      key.extractable !== extractable ||
                      key.algorithm.name !== "HMAC" ||
                      key.algorithm.hash.name !== spec.hash ||
                      key.algorithm.length !== spec.expectedLength ||
                      key.usages.join(",") !== expectedUsages.join(",") ||
                      key[Symbol.toStringTag] !== "CryptoKey"
                    ) {
                      failures.push("shape:" + label);
                    }
                    if (key.algorithm !== key.algorithm || key.algorithm.hash !== key.algorithm.hash) {
                      failures.push("algorithm-cached-wrapper:" + label);
                    }
                    if (key.usages !== key.usages) {
                      failures.push("usages-cached-wrapper:" + label);
                    }
                    if (extractable) {
                      const raw = await subtle.exportKey("raw", key);
                      if (new Uint8Array(raw).byteLength !== spec.expectedLength / 8) {
                        failures.push("raw-length:" + label);
                      }
                      const jwk = await subtle.exportKey("jwk", key);
                      if (
                        jwk.kty !== "oct" ||
                        jwk.ext !== true ||
                        jwk.key_ops.join(",") !== expectedUsages.join(",") ||
                        typeof jwk.k !== "string"
                      ) {
                        failures.push("jwk:" + label);
                      }
                    } else {
                      const exportError = await rejectionName(subtle.exportKey("raw", key));
                      if (exportError !== "InvalidAccessError") {
                        failures.push("unextractable-export:" + label + ":" + exportError);
                      }
                    }
                  }
                }
              }
            }

            globalThis.__cryptoHmacGenerateKeyWptMatrixFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle HMAC generateKey WPT success matrix should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoHmacGenerateKeyWptMatrixFailures)")
        .expect("crypto subtle HMAC generateKey WPT matrix should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_hmac_generate_key_matches_wpt_failure_matrix() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoHmacGenerateKeyWptFailureMatrixFailures = ["pending"];
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
            const algorithmSpecs = [
              { name: "HMAC", hash: "SHA-1", length: 160 },
              { name: "HMAC", hash: "SHA-256", length: 256 },
              { name: "HMAC", hash: "SHA-384", length: 384 },
              { name: "HMAC", hash: "SHA-512", length: 512 },
              { name: "HMAC", hash: "SHA-1" },
              { name: "HMAC", hash: "SHA-256" },
              { name: "HMAC", hash: "SHA-384" },
              { name: "HMAC", hash: "SHA-512" }
            ];

            // Chromium WPT: WebCryptoAPI/generateKey/failures_HMAC.
            // Unsupported algorithm details are reported before semantic
            // usage compatibility, empty dictionaries stay TypeError, invalid
            // recognized usages are SyntaxError, and empty secret-key usages
            // are rejected last after algorithm properties normalize.
            const badAlgorithmUsageCases = validUsages(["decrypt", "sign", "deriveBits"], true);
            for (const usages of badAlgorithmUsageCases) {
              for (const extractable of [false, true, "RED", 7]) {
                const error = await rejectionName(subtle.generateKey(
                  { name: "HMAC", hash: "MD5" },
                  extractable,
                  usages
                ));
                if (error !== "NotSupportedError") {
                  failures.push("bad-algorithm:" + usages.join("+") + ":" + extractable + ":" + error);
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

            for (const algorithm of algorithmSpecs) {
              const algorithmLabel = [
                algorithm.hash,
                algorithm.length === undefined ? "default" : algorithm.length
              ].join(":");
              for (const usages of invalidUsages(["sign", "verify"])) {
                const error = await rejectionName(subtle.generateKey(algorithm, true, usages));
                if (error !== "SyntaxError") {
                  failures.push("bad-usages:" + algorithmLabel + ":" + usages.join("+") + ":" + error);
                }
              }
            }

            for (const algorithm of algorithmSpecs) {
              const algorithmLabel = [
                algorithm.hash,
                algorithm.length === undefined ? "default" : algorithm.length
              ].join(":");
              for (const extractable of [false, true]) {
                const error = await rejectionName(subtle.generateKey(algorithm, extractable, []));
                if (error !== "SyntaxError") {
                  failures.push("empty-usages:" + algorithmLabel + ":" + extractable + ":" + error);
                }
              }
            }

            globalThis.__cryptoHmacGenerateKeyWptFailureMatrixFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle HMAC generateKey WPT failure matrix should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoHmacGenerateKeyWptFailureMatrixFailures)")
        .expect("crypto subtle HMAC generateKey WPT failure matrix should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_hmac_import_lengths_match_chromium_backend() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoHmacImportLengthProbe = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const hex = async (buffer) => Array.from(new Uint8Array(buffer))
              .map((byte) => byte.toString(16).padStart(2, "0"))
              .join("");
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );

            // Chromium backend: components/webcrypto/algorithms/hmac_unittest.cc.
            // HMAC import length is measured in bits. The optional length must
            // map to the same encoded byte length as the key material; if it is
            // not byte-aligned, unused trailing bits in the stored key are
            // zeroed before export/sign/verify observes the key.
            const results = [
              await rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", alg: "HS256", k: "AQIDBAUGBwgJCgsMDQ4PEA" },
                { name: "HMAC" },
                true,
                ["sign"]
              )),
              await rejectionName(subtle.importKey(
                "raw",
                new Uint8Array(0),
                { name: "HMAC", hash: "SHA-1" },
                true,
                ["sign"]
              )),
              await rejectionName(subtle.importKey(
                "raw",
                new Uint8Array(1),
                { name: "HMAC", hash: "SHA-1", length: 0 },
                true,
                ["sign"]
              )),
              await rejectionName(subtle.importKey(
                "raw",
                new Uint8Array(15),
                { name: "HMAC", hash: "SHA-1", length: 128 },
                true,
                ["sign"]
              )),
              await rejectionName(subtle.importKey(
                "raw",
                new Uint8Array(16),
                { name: "HMAC", hash: "SHA-1", length: 120 },
                true,
                ["sign"]
              ))
            ];

            const rawTruncated = await subtle.importKey(
              "raw",
              new Uint8Array([0xb1, 0xff]),
              { name: "HMAC", hash: "SHA-1", length: 12 },
              true,
              ["sign"]
            );
            results.push(`${rawTruncated.algorithm.length}:${await hex(await subtle.exportKey("raw", rawTruncated))}`);

            const jwkTruncated = await subtle.importKey(
              "jwk",
              { kty: "oct", k: "sf8" },
              { name: "HMAC", hash: "SHA-1", length: 12 },
              true,
              ["sign"]
            );
            results.push(`${jwkTruncated.algorithm.length}:${await hex(await subtle.exportKey("raw", jwkTruncated))}`);

            const beyondGenerationCapBytes = new Uint8Array(8193).fill(0xff);
            const beyondGenerationCap = await subtle.importKey(
              "raw",
              beyondGenerationCapBytes,
              { name: "HMAC", hash: "SHA-256", length: 65537 },
              true,
              ["sign"]
            );
            const beyondGenerationCapRaw = new Uint8Array(await subtle.exportKey("raw", beyondGenerationCap));
            results.push([
              beyondGenerationCap.algorithm.length,
              beyondGenerationCapRaw.byteLength,
              beyondGenerationCapRaw[beyondGenerationCapRaw.byteLength - 1].toString(16)
            ].join(":"));

            globalThis.__cryptoHmacImportLengthProbe = results;
          })();
        })()
        "#,
    )
    .expect("crypto subtle HMAC import length probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoHmacImportLengthProbe)")
        .expect("crypto subtle HMAC import length promise chain should settle");

    assert_eq!(
        result,
        r#"["TypeError","DataError","DataError","DataError","DataError","12:b1f0","12:b1f0","65537:8193:80"]"#
    );
}
#[test]
fn crypto_subtle_hmac_jwk_round_trip_supports_stored_secret() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoJwkProbe = [];
          (async () => {
            const encoder = new TextEncoder();
            const key = await crypto.subtle.generateKey(
              { name: "HMAC", hash: { name: "SHA-512" } },
              true,
              ["sign", "verify"]
            );
            const jwk = await crypto.subtle.exportKey("jwk", key);
            localStorage.setItem("client-correlated-secret", JSON.stringify(jwk));
            const stored = JSON.parse(localStorage.getItem("client-correlated-secret"));
            const mismatchedAlgJwk = JSON.parse(JSON.stringify(stored));
            mismatchedAlgJwk.alg = "HS256";
            const rejection = (promise) => promise.then(
              () => "resolved",
              (error) => error.message
            );
            const mismatchedAlg = await rejection(crypto.subtle.importKey(
              "jwk",
              mismatchedAlgJwk,
              { name: "HMAC", hash: { name: "SHA-512" } },
              true,
              ["sign"]
            ));
            const imported = await crypto.subtle.importKey(
              "jwk",
              stored,
              { name: "HMAC", hash: { name: "SHA-512" } },
              true,
              ["sign", "verify"]
            );
            const signature = await crypto.subtle.sign(
              "HMAC",
              imported,
              encoder.encode("?model=gpt")
            );
            const verified = await crypto.subtle.verify(
            { name: "HMAC", hash: { name: "SHA-512" } },
              imported,
              signature,
              encoder.encode("?model=gpt")
            );
            const raw = await crypto.subtle.exportKey("raw", imported);
            globalThis.__cryptoJwkProbe = [
              jwk.kty,
              jwk.alg,
              typeof jwk.k,
              String(jwk.k.length),
              jwk.key_ops.join(","),
              String(jwk.ext),
              imported.algorithm.name,
              imported.algorithm.hash.name,
              String(imported.algorithm.length),
              String(new Uint8Array(signature).length),
              String(raw.byteLength),
              String(verified),
              mismatchedAlg
            ];
          })();
        })()
        "#,
    )
    .expect("crypto subtle JWK probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoJwkProbe)")
        .expect("crypto subtle JWK promise chain should settle");

    assert_eq!(
        result,
        r#"["oct","HS512","string","171","sign,verify","true","HMAC","SHA-512","1024","64","128","true","DataError"]"#
    );
}
#[test]
fn crypto_subtle_hmac_matches_chromium_wpt_sign_verify_vectors() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoHmacWptVectorFailures = ["pending"];
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
            const hexBytes = (value) => value.length === 0
              ? new Uint8Array([])
              : new Uint8Array(value.match(/../g).map((byte) => parseInt(byte, 16)));
            const asBufferSource = (bytes, arrayBuffer) => arrayBuffer ? bytes.buffer : bytes;
            // Chromium WPT: WebCryptoAPI/sign_verify/hmac_vectors.js
            const plaintext = new Uint8Array([
              95, 77, 186, 79, 50, 12, 12, 232, 118, 114, 90, 252, 229, 251, 210, 91,
              248, 62, 90, 113, 37, 160, 140, 175, 231, 60, 62, 186, 196, 33, 119, 157,
              249, 213, 93, 24, 12, 58, 233, 148, 38, 69, 225, 216, 47, 238, 140, 157,
              41, 75, 60, 177, 160, 138, 153, 49, 32, 27, 60, 14, 129, 252, 71, 202,
              207, 131, 21, 162, 175, 102, 50, 65, 19, 195, 182, 98, 48, 195, 70, 8,
              196, 244, 89, 54, 52, 206, 2, 178, 103, 54, 34, 119, 240, 168, 64, 202,
              116, 188, 61, 26, 98, 54, 149, 44, 94, 215, 170, 248, 168, 254, 203,
              221, 250, 117, 132, 230, 151, 140, 234, 93, 42, 91, 159, 183, 241, 180,
              140, 139, 11, 229, 138, 48, 82, 2, 117, 77, 131, 118, 16, 115, 116, 121,
              60, 240, 38, 170, 238, 83, 0, 114, 125, 131, 108, 215, 30, 113, 179, 69,
              221, 178, 228, 68, 70, 255, 197, 185, 1, 99, 84, 19, 137, 13, 145, 14,
              163, 128, 152, 74, 144, 25, 16, 49, 50, 63, 22, 219, 204, 157, 107, 225,
              104, 184, 72, 133, 56, 76, 160, 62, 18, 96, 10, 193, 194, 72, 2, 138,
              243, 114, 108, 201, 52, 99, 136, 46, 168, 192, 42, 171
            ]);
            const vectors = [
              {
                hash: "SHA-1",
                key: [71, 162, 7, 70, 209, 113, 121, 219, 101, 224, 167, 157, 237, 255, 199, 253, 241, 129, 8, 27],
                signature: [5, 51, 144, 42, 153, 248, 82, 78, 229, 10, 240, 29, 56, 222, 220, 225, 51, 217, 140, 160]
              },
              {
                hash: "SHA-256",
                key: [229, 136, 236, 8, 17, 70, 61, 118, 114, 65, 223, 16, 116, 180, 122, 228, 7, 27, 81, 242, 206, 54, 83, 123, 166, 156, 205, 195, 253, 194, 183, 168],
                signature: [133, 164, 12, 234, 46, 7, 140, 40, 39, 163, 149, 63, 251, 102, 194, 123, 41, 26, 71, 43, 13, 112, 160, 0, 11, 69, 216, 35, 128, 62, 235, 84]
              },
              {
                hash: "SHA-384",
                key: [107, 29, 162, 142, 171, 31, 88, 42, 217, 113, 142, 255, 224, 94, 35, 213, 253, 44, 152, 119, 162, 217, 68, 63, 144, 190, 192, 147, 190, 206, 46, 167, 210, 53, 76, 208, 189, 197, 225, 71, 210, 233, 0, 147, 115, 73, 68, 136],
                signature: [33, 124, 61, 80, 240, 186, 154, 109, 110, 174, 30, 253, 215, 165, 24, 254, 46, 56, 128, 181, 130, 164, 13, 6, 30, 144, 153, 193, 224, 38, 239, 88, 130, 84, 139, 93, 92, 236, 221, 85, 152, 217, 155, 107, 111, 48, 87, 255]
              },
              {
                hash: "SHA-512",
                key: [93, 204, 53, 148, 67, 170, 246, 82, 250, 19, 117, 214, 179, 230, 31, 220, 242, 155, 180, 162, 139, 213, 211, 220, 250, 64, 248, 47, 144, 107, 178, 128, 4, 85, 219, 3, 181, 211, 31, 185, 114, 161, 90, 109, 1, 3, 162, 78, 86, 209, 86, 161, 25, 192, 229, 161, 233, 42, 68, 195, 197, 101, 124, 249],
                signature: [97, 251, 39, 140, 63, 251, 12, 206, 43, 241, 207, 114, 61, 223, 216, 239, 31, 147, 28, 12, 97, 140, 37, 144, 115, 36, 96, 89, 57, 227, 249, 162, 198, 244, 175, 105, 11, 218, 52, 7, 220, 47, 87, 112, 246, 160, 164, 75, 149, 77, 100, 163, 50, 227, 238, 8, 33, 171, 248, 43, 127, 62, 153, 193]
              }
            ];

            for (const vector of vectors) {
              const label = vector.hash;
              const signature = new Uint8Array(vector.signature);
              const key = await subtle.importKey(
                "raw",
                new Uint8Array(vector.key),
                { name: "HMAC", hash: vector.hash },
                false,
                ["verify", "sign"]
              );
              if (key.type !== "secret" || key.algorithm.hash.name !== vector.hash) {
                failures.push(label + ":key-shape");
              }
              if (!await subtle.verify("HMAC", key, signature, plaintext)) {
                failures.push(label + ":verify");
              }
              if (!sameBytes(await subtle.sign("HMAC", key, plaintext), signature)) {
                failures.push(label + ":sign");
              }

              const wrongPlaintext = new Uint8Array(plaintext);
              wrongPlaintext[0] = 255 - wrongPlaintext[0];
              if (await subtle.verify("HMAC", key, signature, wrongPlaintext)) {
                failures.push(label + ":wrong-plaintext");
              }

              const wrongSignature = new Uint8Array(signature);
              wrongSignature[0] = 255 - wrongSignature[0];
              if (await subtle.verify("HMAC", key, wrongSignature, plaintext)) {
                failures.push(label + ":wrong-signature");
              }
              if (await subtle.verify("HMAC", key, signature.slice(1), plaintext)) {
                failures.push(label + ":short-signature");
              }
              if (await subtle.verify("HMAC", key, new Uint8Array(), plaintext)) {
                failures.push(label + ":empty-signature");
              }
              const oversizedSignature = new Uint8Array(signature.byteLength + 1);
              oversizedSignature.set(signature);
              oversizedSignature[oversizedSignature.byteLength - 1] = 0xff;
              if (await subtle.verify("HMAC", key, oversizedSignature, plaintext)) {
                failures.push(label + ":oversized-signature");
              }

              const afterCallPlaintext = new Uint8Array(plaintext);
              const pendingSign = subtle.sign("HMAC", key, afterCallPlaintext);
              afterCallPlaintext[0] = 255 - afterCallPlaintext[0];
              if (!sameBytes(await pendingSign, signature)) {
                failures.push(label + ":sign-buffer-snapshot");
              }

              const duringCallPlaintext = new Uint8Array(plaintext);
              duringCallPlaintext[0] = 255 - duringCallPlaintext[0];
              const duringCallSigned = await subtle.sign({
                get name() {
                  duringCallPlaintext[0] = plaintext[0];
                  return "HMAC";
                }
              }, key, duringCallPlaintext);
              if (!sameBytes(duringCallSigned, signature)) {
                failures.push(label + ":sign-normalization-snapshot");
              }

              // WPT WebCryptoAPI/sign_verify/hmac.js: operation
              // BufferSources are converted after algorithm normalization, so
              // name getter side effects during the call affect the signature
              // and data snapshots.
              const duringCallSignature = new Uint8Array(signature);
              const duringCallVerifyPlaintext = new Uint8Array(plaintext);
              duringCallSignature[0] = 255 - duringCallSignature[0];
              duringCallVerifyPlaintext[0] = 255 - duringCallVerifyPlaintext[0];
              const duringCallVerified = await subtle.verify({
                get name() {
                  duringCallSignature[0] = signature[0];
                  duringCallVerifyPlaintext[0] = plaintext[0];
                  return "HMAC";
                }
              }, key, duringCallSignature, duringCallVerifyPlaintext);
              if (!duringCallVerified) {
                failures.push(label + ":verify-normalization-snapshot");
              }

              const afterCallSignature = new Uint8Array(signature);
              const pendingSignatureVerify = subtle.verify("HMAC", key, afterCallSignature, plaintext);
              afterCallSignature[0] = 255 - afterCallSignature[0];
              if (!await pendingSignatureVerify) {
                failures.push(label + ":verify-signature-buffer-snapshot");
              }

              const afterCallVerifyPlaintext = new Uint8Array(plaintext);
              const pendingPlaintextVerify = subtle.verify("HMAC", key, signature, afterCallVerifyPlaintext);
              afterCallVerifyPlaintext[0] = 255 - afterCallVerifyPlaintext[0];
              if (!await pendingPlaintextVerify) {
                failures.push(label + ":verify-plaintext-buffer-snapshot");
              }

              const signOnly = await subtle.importKey(
                "raw",
                new Uint8Array(vector.key),
                { name: "HMAC", hash: vector.hash },
                false,
                ["sign"]
              );
              const noVerify = await rejectionName(
                subtle.verify("HMAC", signOnly, signature, plaintext)
              );
              if (noVerify !== "InvalidAccessError") {
                failures.push(label + ":no-verify-usage:" + noVerify);
              }
            }
            // Chromium legacy test: crypto/subtle/hmac/sign-verify.html.
            // These CAVP/OpenSSL vectors cover raw-key import shape, sign,
            // verify, ArrayBuffer/TypedArray inputs, and truncated MAC
            // verification returning false.
            const legacyHmacVectors = [
              {
                hash: "SHA-1",
                key: "00",
                message: "",
                mac: "fbdb1d1b18aa6c08324b7d64b71fb76370690e1d"
              },
              {
                hash: "SHA-256",
                key: "00",
                message: "",
                mac: "b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad"
              },
              {
                hash: "SHA-1",
                key: "59785928d72516e31272",
                message: "a3ce8899df1022e8d2d539b47bf0e309c66f84095e21438ec355bf119ce5fdcb4e73a619cdf36f25b369d8c38ff419997f0c59830108223606e31223483fd39edeaa4d3f0d21198862d239c9fd26074130ff6c86493f5227ab895c8f244bd42c7afce5d147a20a590798c68e708e964902d124dadecdbda9dbd0051ed710e9bf",
                mac: "3c8162589aafaee024fc9a5ca50dd2336fe3eb28"
              },
              {
                hash: "SHA-1",
                key: "ceb9aedf8d6efcf0ae52bea0fa99a9e26ae81bacea0cff4d5eecf201e3bca3c3577480621b818fd717ba99d6ff958ea3d59b2527b019c343bb199e648090225867d994607962f5866aa62930d75b58f6",
                message: "99958aa459604657c7bf6e4cdfcc8785f0abf06ffe636b5b64ecd931bd8a456305592421fc28dbcccb8a82acea2be8e54161d7a78e0399a6067ebaca3f2510274dc9f92f2c8ae4265eec13d7d42e9f8612d7bc258f913ecb5a3a5c610339b49fb90e9037b02d684fc60da835657cb24eab352750c8b463b1a8494660d36c3ab2",
                mac: "4ac41ab89f625c60125ed65ffa958c6b490ea670"
              },
              {
                hash: "SHA-256",
                key: "9779d9120642797f1747025d5b22b7ac607cab08e1758f2f3a46c8be1e25c53b8c6a8f58ffefa176",
                message: "b1689c2591eaf3c9e66070f8a77954ffb81749f1b00346f9dfe0b2ee905dcc288baf4a92de3f4001dd9f44c468c3d07d6c6ee82faceafc97c2fc0fc0601719d2dcd0aa2aec92d1b0ae933c65eb06a03c9c935c2bad0459810241347ab87e9f11adb30415424c6c7f5f22a003b8ab8de54f6ded0e3ab9245fa79568451dfa258e",
                mac: "769f00d3e6a6cc1fb426a14a4f76c6462e6149726e0dee0ec0cf97a16605ac8b"
              },
              {
                hash: "SHA-256",
                key: "4b7ab133efe99e02fc89a28409ee187d579e774f4cba6fc223e13504e3511bef8d4f638b9aca55d4a43b8fbd64cf9d74dcc8c9e8d52034898c70264ea911a3fd70813fa73b083371289b",
                message: "138efc832c64513d11b9873c6fd4d8a65dbf367092a826ddd587d141b401580b798c69025ad510cff05fcfbceb6cf0bb03201aaa32e423d5200925bddfadd418d8e30e18050eb4f0618eb9959d9f78c1157d4b3e02cd5961f138afd57459939917d9144c95d8e6a94c8f6d4eef3418c17b1ef0b46c2a7188305d9811dccb3d99",
                mac: "4f1ee7cb36c58803a8721d4ac8c4cf8cae5d8832392eed2a96dc59694252801b"
              }
            ];
            for (const vector of legacyHmacVectors) {
              const label = `legacy:${vector.hash}:${vector.key.length}`;
              const keyBytes = hexBytes(vector.key);
              const messageBytes = hexBytes(vector.message);
              const macBytes = hexBytes(vector.mac);
              const key = await subtle.importKey(
                "raw",
                keyBytes,
                { name: "HMAC", hash: { name: vector.hash } },
                false,
                ["sign", "verify"]
              );
              if (
                key.type !== "secret" ||
                key.extractable !== false ||
                key.algorithm.name !== "HMAC" ||
                key.algorithm.hash.name !== vector.hash ||
                key.algorithm.length !== keyBytes.byteLength * 8 ||
                key.usages.join(",") !== "sign,verify"
              ) {
                failures.push(label + ":key-shape");
              }
              if (!sameBytes(await subtle.sign("HMAC", key, messageBytes), macBytes)) {
                failures.push(label + ":sign");
              }
              for (const signatureArrayBuffer of [true, false]) {
                for (const dataArrayBuffer of [true, false]) {
                  const verified = await subtle.verify(
                    "HMAC",
                    key,
                    asBufferSource(hexBytes(vector.mac), signatureArrayBuffer),
                    asBufferSource(hexBytes(vector.message), dataArrayBuffer)
                  );
                  if (!verified) {
                    failures.push(`${label}:verify:${signatureArrayBuffer}:${dataArrayBuffer}`);
                  }
                }
              }
              if (await subtle.verify("HMAC", key, macBytes.slice(0, macBytes.byteLength - 1), messageBytes)) {
                failures.push(label + ":truncated");
              }
              if (await subtle.verify("HMAC", key, new Uint8Array(), messageBytes)) {
                failures.push(label + ":empty-signature");
              }
              const oversizedMac = new Uint8Array(macBytes.byteLength + 1);
              oversizedMac.set(macBytes);
              oversizedMac[oversizedMac.byteLength - 1] = 0xff;
              if (await subtle.verify("HMAC", key, oversizedMac, messageBytes)) {
                failures.push(label + ":oversized-signature");
              }
            }
            globalThis.__cryptoHmacWptVectorFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle HMAC WPT vector probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoHmacWptVectorFailures)")
        .expect("crypto subtle HMAC WPT vector promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_hmac_sign_verify_bad_parameters_match_chromium() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoHmacBadParametersProbe = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const key = await subtle.importKey(
              "raw",
              new Uint8Array(16),
              { name: "HMAC", hash: { name: "sha-1" } },
              true,
              ["sign", "verify"]
            );
            const data = new Uint8Array([104, 101, 108, 108, 111]);
            const hmac = { name: "HMAC", hash: { name: "sha-1" } };

            // Chromium legacy test:
            // crypto/subtle/sign-verify-badParameters.html.
            globalThis.__cryptoHmacBadParametersProbe = await Promise.all([
              rejectionName(subtle.verify(hmac, key, null, data)),
              rejectionName(subtle.verify(hmac, key, "a", data)),
              rejectionName(subtle.verify(hmac, key, [], data)),
              rejectionName(subtle.sign({ name: "sha-1" }, key, data)),
              rejectionName(subtle.sign({ name: "AES-CBC" }, key, data))
            ]);
          })();
        })()
        "#,
    )
    .expect("crypto subtle HMAC bad-parameter probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoHmacBadParametersProbe)")
        .expect("crypto subtle HMAC bad-parameter promise chain should settle");

    assert_eq!(
        result,
        r#"["TypeError","TypeError","TypeError","NotSupportedError","NotSupportedError"]"#
    );
}
#[test]
fn crypto_subtle_hmac_import_export_and_generate_key_match_wpt_matrix() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoHmacWptMatrixFailures = ["pending"];
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
              { bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]), k: "AQIDBAUGBwgJCgsMDQ4PEA" },
              { bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24]), k: "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcY" },
              { bytes: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32]), k: "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA" }
            ];
            const hashCases = [
              { hash: "SHA-1", alg: "HS1", explicitLength: 160, defaultLength: 512 },
              { hash: "SHA-256", alg: "HS256", explicitLength: 256, defaultLength: 512 },
              { hash: "SHA-384", alg: "HS384", explicitLength: 384, defaultLength: 1024 },
              { hash: "SHA-512", alg: "HS512", explicitLength: 512, defaultLength: 1024 }
            ];
            const usageCases = [["sign"], ["verify"], ["sign", "verify"]];

            const duplicateGenerated = await subtle.generateKey(
              { name: "HMAC", hash: "SHA-256" },
              true,
              ["sign", "verify", "sign", "verify"]
            );
            if (duplicateGenerated.usages.join(",") !== "sign,verify") {
              failures.push("duplicate-usages:generate");
            }
            const duplicateRaw = await subtle.importKey(
              "raw",
              rawCases[0].bytes,
              { name: "HMAC", hash: "SHA-256" },
              true,
              ["verify", "verify", "sign", "verify"]
            );
            const duplicateRawJwk = await subtle.exportKey("jwk", duplicateRaw);
            if (
              duplicateRaw.usages.join(",") !== "sign,verify" ||
              duplicateRawJwk.key_ops.join(",") !== "sign,verify"
            ) {
              failures.push("duplicate-usages:import-export");
            }
            // Chromium: crypto/subtle/jwk-import-use-values.html.  JWK `use`
            // constrains the key family, while duplicate `key_ops` is invalid.
            const useConstrainedJwk = await subtle.importKey(
              "jwk",
              { kty: "oct", k: rawCases[0].k, alg: "HS256", use: "sig" },
              { name: "HMAC", hash: "SHA-256" },
              true,
              ["sign"]
            );
            if (useConstrainedJwk.usages.join(",") !== "sign") {
              failures.push("jwk-use:sig");
            }
            const jwkUseErrors = await Promise.all([
              rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", k: rawCases[0].k, alg: "HS256", use: "enc" },
                { name: "HMAC", hash: "SHA-256" },
                true,
                ["sign"]
              )),
              rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", k: rawCases[0].k, alg: "HS256", key_ops: ["sign", "sign"] },
                { name: "HMAC", hash: "SHA-256" },
                true,
                ["sign"]
              )),
              rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", k: rawCases[0].k, alg: "HS256", key_ops: ["verify"] },
                { name: "HMAC", hash: "SHA-256" },
                true,
                ["sign"]
              )),
              rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", k: rawCases[0].k, alg: "HS256", use: "sig", key_ops: ["sign", "verify", "encrypt"] },
                { name: "HMAC", hash: "SHA-256" },
                true,
                ["sign"]
              ))
            ]);
            if (jwkUseErrors.join(",") !== "DataError,DataError,DataError,DataError") {
              failures.push("jwk-use-key-ops-errors:" + jwkUseErrors.join(","));
            }

            for (const hashCase of hashCases) {
              for (const keyCase of rawCases) {
                for (const usages of usageCases) {
                  for (const extractable of [true, false]) {
                    const rawKey = await subtle.importKey(
                      "raw",
                      keyCase.bytes,
                      { name: "HMAC", hash: hashCase.hash },
                      extractable,
                      usages
                    );
                    if (
                      rawKey.type !== "secret" ||
                      rawKey.extractable !== extractable ||
                      rawKey.algorithm.name !== "HMAC" ||
                      rawKey.algorithm.hash.name !== hashCase.hash ||
                      rawKey.algorithm.length !== keyCase.bytes.byteLength * 8 ||
                      rawKey.usages.join(",") !== usages.join(",")
                    ) {
                      failures.push(hashCase.hash + ":raw-shape:" + keyCase.bytes.byteLength + ":" + usages.join("+"));
                    }
                    if (extractable && !sameBytes(await subtle.exportKey("raw", rawKey), keyCase.bytes)) {
                      failures.push(hashCase.hash + ":raw-round-trip:" + keyCase.bytes.byteLength);
                    }

                    const jwkData = { kty: "oct", k: keyCase.k, alg: hashCase.alg };
                    const jwkKey = await subtle.importKey(
                      "jwk",
                      jwkData,
                      { name: "HMAC", hash: hashCase.hash },
                      extractable,
                      usages
                    );
                    if (
                      jwkKey.type !== "secret" ||
                      jwkKey.algorithm.hash.name !== hashCase.hash ||
                      jwkKey.algorithm.length !== keyCase.bytes.byteLength * 8
                    ) {
                      failures.push(hashCase.hash + ":jwk-shape:" + keyCase.bytes.byteLength);
                    }
                    if (extractable) {
                      const exported = await subtle.exportKey("jwk", jwkKey);
                      if (
                        exported.kty !== "oct" ||
                        exported.k !== keyCase.k ||
                        exported.alg !== hashCase.alg ||
                        exported.ext !== true ||
                        exported.key_ops.join(",") !== usages.join(",")
                      ) {
                        failures.push(hashCase.hash + ":jwk-round-trip:" + keyCase.bytes.byteLength + ":" + usages.join("+"));
                      }
                    }
                  }
                  const emptyRaw = await rejectionName(subtle.importKey(
                    "raw",
                    keyCase.bytes,
                    { name: "HMAC", hash: hashCase.hash },
                    true,
                    []
                  ));
                  const emptyJwk = await rejectionName(subtle.importKey(
                    "jwk",
                    { kty: "oct", k: keyCase.k, alg: hashCase.alg },
                    { name: "HMAC", hash: hashCase.hash },
                    true,
                    []
                  ));
                  if (emptyRaw !== "SyntaxError" || emptyJwk !== "SyntaxError") {
                    failures.push(hashCase.hash + ":empty-usages:" + emptyRaw + ":" + emptyJwk);
                  }
                }
              }

              const generated = await subtle.generateKey(
                { name: "HMAC", hash: hashCase.hash, length: hashCase.explicitLength },
                true,
                ["sign", "verify"]
              );
              if (
                generated.algorithm.hash.name !== hashCase.hash ||
                generated.algorithm.length !== hashCase.explicitLength ||
                new Uint8Array(await subtle.exportKey("raw", generated)).byteLength !== hashCase.explicitLength / 8
              ) {
                failures.push(hashCase.hash + ":generate-explicit-length");
              }
              const defaultGenerated = await subtle.generateKey(
                { name: "HMAC", hash: hashCase.hash },
                true,
                ["sign"]
              );
              if (
                defaultGenerated.algorithm.length !== hashCase.defaultLength ||
                new Uint8Array(await subtle.exportKey("raw", defaultGenerated)).byteLength !== hashCase.defaultLength / 8
              ) {
                failures.push(hashCase.hash + ":generate-default-length");
              }
            }
            globalThis.__cryptoHmacWptMatrixFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle HMAC WPT matrix probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoHmacWptMatrixFailures)")
        .expect("crypto subtle HMAC WPT matrix promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_hmac_kdf_import_export_matches_wpt_symmetric_matrix() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoSupportedSymmetricImportMatrixFailures = ["pending"];
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

            // Chromium WPT: WebCryptoAPI/import_export/symmetric_importKey.js.
            const hmacAlgorithms = [
              { name: "HMAC", hash: "SHA-1", alg: "HS1", usages: ["sign", "verify"] },
              { name: "HMAC", hash: "SHA-256", alg: "HS256", usages: ["sign", "verify"] },
              { name: "HMAC", hash: "SHA-384", alg: "HS384", usages: ["sign", "verify"] },
              { name: "HMAC", hash: "SHA-512", alg: "HS512", usages: ["sign", "verify"] }
            ];
            for (const algorithm of hmacAlgorithms) {
              for (const keyCase of rawCases) {
                for (const format of ["raw", "jwk"]) {
                  const keyData = format === "raw"
                    ? keyCase.bytes
                    : { kty: "oct", k: keyCase.k, alg: algorithm.alg };
                  for (const extractable of [true, false]) {
                    for (const usages of validUsages(algorithm.usages)) {
                      const key = await subtle.importKey(
                        format,
                        keyData,
                        { name: algorithm.name, hash: algorithm.hash },
                        extractable,
                        usages
                      );
                      if (
                        key.constructor !== CryptoKey ||
                        key.type !== "secret" ||
                        key.extractable !== extractable ||
                        key.algorithm.name !== "HMAC" ||
                        key.algorithm.hash.name !== algorithm.hash ||
                        key.algorithm.length !== keyCase.bits ||
                        !sameUsageSet(key.usages, usages) ||
                        key[Symbol.toStringTag] !== "CryptoKey"
                      ) {
                        failures.push([
                          "hmac-shape",
                          algorithm.hash,
                          format,
                          keyCase.bits,
                          extractable,
                          usages.join("+"),
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
                          exportedJwk.alg !== algorithm.alg ||
                          exportedJwk.ext !== true ||
                          !sameUsageSet(exportedJwk.key_ops, usages)
                        ) {
                          failures.push([
                            "hmac-export",
                            algorithm.hash,
                            format,
                            keyCase.bits,
                            exportedRaw.byteLength,
                            exportedJwk.alg,
                            exportedJwk.key_ops.join("+")
                          ].join(":"));
                        }
                      } else {
                        const rawError = await rejectionName(subtle.exportKey("raw", key));
                        const jwkError = await rejectionName(subtle.exportKey("jwk", key));
                        if (rawError !== "InvalidAccessError" || jwkError !== "InvalidAccessError") {
                          failures.push("hmac-unextractable:" + algorithm.hash + ":" + format + ":" + rawError + ":" + jwkError);
                        }
                      }
                    }

                    const emptyError = await rejectionName(subtle.importKey(
                      format,
                      keyData,
                      { name: algorithm.name, hash: algorithm.hash },
                      extractable,
                      []
                    ));
                    if (emptyError !== "SyntaxError") {
                      failures.push("hmac-empty-usages:" + algorithm.hash + ":" + format + ":" + keyCase.bits + ":" + emptyError);
                    }
                  }
                }
              }
            }

            const kdfAlgorithms = [
              { name: "HKDF", usages: ["deriveBits", "deriveKey"] },
              { name: "PBKDF2", usages: ["deriveBits", "deriveKey"] }
            ];
            for (const algorithm of kdfAlgorithms) {
              for (const keyCase of rawCases) {
                for (const usages of validUsages(algorithm.usages)) {
                  const key = await subtle.importKey(
                    "raw",
                    keyCase.bytes,
                    { name: algorithm.name },
                    false,
                    usages
                  );
                  if (
                    key.constructor !== CryptoKey ||
                    key.type !== "secret" ||
                    key.extractable !== false ||
                    key.algorithm.name !== algorithm.name ||
                    !sameUsageSet(key.usages, usages) ||
                    key[Symbol.toStringTag] !== "CryptoKey"
                  ) {
                    failures.push([
                      "kdf-shape",
                      algorithm.name,
                      keyCase.bits,
                      usages.join("+"),
                      key.algorithm.name,
                      key.usages.join("+")
                    ].join(":"));
                  }
                  const rawError = await rejectionName(subtle.exportKey("raw", key));
                  if (rawError !== "InvalidAccessError") {
                    failures.push("kdf-unextractable:" + algorithm.name + ":" + keyCase.bits + ":" + rawError);
                  }
                }
                const emptyError = await rejectionName(subtle.importKey(
                  "raw",
                  keyCase.bytes,
                  { name: algorithm.name },
                  false,
                  []
                ));
                if (emptyError !== "SyntaxError") {
                  failures.push("kdf-empty-usages:" + algorithm.name + ":" + keyCase.bits + ":" + emptyError);
                }
              }
            }

            globalThis.__cryptoSupportedSymmetricImportMatrixFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle supported symmetric import matrix should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoSupportedSymmetricImportMatrixFailures)")
        .expect("crypto subtle supported symmetric import matrix promise chain should settle");

    assert_eq!(result, "[]");
}
