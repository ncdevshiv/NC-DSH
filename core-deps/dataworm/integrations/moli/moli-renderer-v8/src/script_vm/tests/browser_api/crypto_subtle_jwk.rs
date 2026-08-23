use super::*;

#[test]
fn crypto_subtle_jwk_import_uses_webidl_dictionary_conversion() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoJwkWebIdlFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            // Chromium: crypto/subtle/import-jwk.html. JsonWebKey is a
            // WebIDL dictionary, so null/undefined become an empty dictionary
            // and member values use WebIDL DOMString/boolean conversion.
            const hmac256 = { name: "HMAC", hash: { name: "SHA-256" } };
            const hmacKey = "ahjkn-_387fgnsibf23qsvahjkn-_387fgnsibf23qs";
            const aesKey = "AQIDBAUGBwgJCgsMDQ4PEA";
            const emptyDictionaryErrors = await Promise.all([
              rejectionName(subtle.importKey("jwk", null, { name: "AES-CBC" }, true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", undefined, { name: "AES-CBC" }, true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", {}, { name: "AES-CBC" }, true, ["encrypt"])),
              rejectionName(subtle.importKey("jwk", 7, { name: "AES-CBC" }, true, ["encrypt"]))
            ]);
            if (emptyDictionaryErrors.join(",") !== "DataError,DataError,DataError,TypeError") {
              failures.push("empty-dictionary-errors:" + emptyDictionaryErrors.join(","));
            }

            const extStringKey = await subtle.importKey(
              "jwk",
              { kty: "oct", alg: "HS256", use: "sig", ext: "false", k: hmacKey },
              hmac256,
              false,
              ["sign"]
            );
            if (
              !(extStringKey instanceof CryptoKey) ||
              extStringKey.extractable !== false ||
              extStringKey.algorithm.name !== "HMAC" ||
              extStringKey.usages.join(",") !== "sign"
            ) {
              failures.push("ext-string-boolean-conversion");
            }

            const numericKKey = await subtle.importKey(
              "jwk",
              { kty: "oct", alg: "HS256", use: "sig", ext: false, k: 1258 },
              hmac256,
              false,
              ["sign"]
            );
            if (
              !(numericKKey instanceof CryptoKey) ||
              numericKKey.algorithm.name !== "HMAC" ||
              numericKKey.usages.join(",") !== "sign"
            ) {
              failures.push("numeric-k-domstring-conversion");
            }

            const convertedMemberErrors = await Promise.all([
              rejectionName(subtle.importKey(
                "jwk",
                { kty: 1, alg: "HS256", use: "sig", ext: false, k: hmacKey },
                hmac256,
                false,
                ["sign"]
              )),
              rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", alg: 1, use: "enc", ext: false, k: aesKey },
                { name: "AES-CBC" },
                false,
                ["encrypt"]
              )),
              rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", alg: "HS256", use: 1, ext: false, k: hmacKey },
                hmac256,
                false,
                ["sign"]
              )),
              rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", alg: "A128CBC", use: "enc", ext: false, k: "1234" },
                { name: "AES-CBC" },
                false,
                ["encrypt"]
              )),
              rejectionName(subtle.importKey(
                "jwk",
                { kty: "oct", alg: "HS256", use: "sig", ext: false, k: "ahjkn23387f+nsibf23qsvahjkn37387fgnsibf23qs" },
                hmac256,
                false,
                ["sign"]
              ))
            ]);
            if (convertedMemberErrors.join(",") !== "DataError,DataError,DataError,DataError,DataError") {
              failures.push("converted-member-errors:" + convertedMemberErrors.join(","));
            }

            globalThis.__cryptoJwkWebIdlFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle JWK WebIDL probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoJwkWebIdlFailures)")
        .expect("crypto subtle JWK WebIDL promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_jwk_import_use_values_match_chromium_legacy_matrix() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoJwkImportUseValueFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const aesBase = {
              alg: "A128CBC",
              ext: true,
              kty: "oct",
              k: "jnOw99oOZFLIEPMrgJB55Q"
            };
            const hmacBase = {
              alg: "HS256",
              ext: true,
              kty: "oct",
              k: "ahjkn-_387fgnsibf23qsvahjkn-_387fgnsibf23qs"
            };
            const buildJwk = (base, jwkUsages) => {
              const jwk = { ...base };
              if ("key_ops" in jwkUsages) {
                jwk.key_ops = jwkUsages.key_ops.slice();
              } else {
                jwk.use = jwkUsages.use;
              }
              return jwk;
            };
            const expectImport = async (family, label, jwkUsages, importUsages, expectedUsages) => {
              const base = family === "AES-CBC" ? aesBase : hmacBase;
              const algorithm = family === "AES-CBC"
                ? { name: "AES-CBC" }
                : { name: "HMAC", hash: { name: "SHA-256" } };
              try {
                const key = await subtle.importKey(
                  "jwk",
                  buildJwk(base, jwkUsages),
                  algorithm,
                  true,
                  importUsages
                );
                if (expectedUsages === null) {
                  failures.push(`${label}:resolved:${key.usages.join(",")}`);
                  return;
                }
                if (
                  key.type !== "secret" ||
                  key.algorithm.name !== family ||
                  key.usages.join(",") !== expectedUsages.join(",")
                ) {
                  failures.push(`${label}:shape:${key.algorithm.name}:${key.usages.join(",")}`);
                }
              } catch (error) {
                if (expectedUsages !== null || error.name !== "DataError") {
                  failures.push(`${label}:rejected:${error.name}`);
                }
              }
            };

            // Chromium legacy test: crypto/subtle/jwk-import-use-values.html.
            // This pins `key_ops` as a required superset of requested usages,
            // duplicate `key_ops` as DataError, `use` as a broad key-family
            // mask, and Chromium's quirk that distinct unknown key_ops strings
            // are ignored rather than rejected.
            const cases = [
              ["aes-dup", "AES-CBC", { key_ops: ["encrypt", "encrypt"] }, ["encrypt"], null],
              ["aes-encrypt-ok", "AES-CBC", { key_ops: ["encrypt"] }, ["encrypt"], ["encrypt"]],
              ["aes-encrypt-missing", "AES-CBC", { key_ops: ["encrypt"] }, ["decrypt"], null],
              ["aes-decrypt-ok", "AES-CBC", { key_ops: ["decrypt"] }, ["decrypt"], ["decrypt"]],
              ["aes-decrypt-missing", "AES-CBC", { key_ops: ["decrypt"] }, ["encrypt"], null],
              ["aes-enc-dec-ok", "AES-CBC", { key_ops: ["encrypt", "decrypt"] }, ["encrypt", "decrypt"], ["encrypt", "decrypt"]],
              ["aes-enc-dec-subset", "AES-CBC", { key_ops: ["encrypt", "decrypt"] }, ["encrypt"], ["encrypt"]],
              ["aes-unwrap-missing", "AES-CBC", { key_ops: ["encrypt", "decrypt"] }, ["unwrapKey"], null],
              ["aes-wrap-ok", "AES-CBC", { key_ops: ["wrapKey"] }, ["wrapKey"], ["wrapKey"]],
              ["aes-wrap-missing", "AES-CBC", { key_ops: ["wrapKey"] }, ["unwrapKey"], null],
              ["aes-unwrap-ok", "AES-CBC", { key_ops: ["unwrapKey"] }, ["unwrapKey"], ["unwrapKey"]],
              ["aes-wrap-unwrap-ok", "AES-CBC", { key_ops: ["wrapKey", "unwrapKey"] }, ["unwrapKey", "wrapKey"], ["wrapKey", "unwrapKey"]],
              ["aes-three-ok", "AES-CBC", { key_ops: ["encrypt", "decrypt", "wrapKey"] }, ["decrypt", "encrypt", "wrapKey"], ["encrypt", "decrypt", "wrapKey"]],
              ["aes-use-enc-all", "AES-CBC", { use: "enc" }, ["decrypt", "encrypt", "unwrapKey", "wrapKey"], ["encrypt", "decrypt", "wrapKey", "unwrapKey"]],
              ["aes-use-enc-subset-a", "AES-CBC", { use: "enc" }, ["decrypt", "encrypt", "unwrapKey"], ["encrypt", "decrypt", "unwrapKey"]],
              ["aes-use-enc-subset-b", "AES-CBC", { use: "enc" }, ["decrypt", "encrypt", "unwrapKey"], ["encrypt", "decrypt", "unwrapKey"]],
              ["hmac-sign-ok", "HMAC", { key_ops: ["sign"] }, ["sign"], ["sign"]],
              ["hmac-sign-missing", "HMAC", { key_ops: ["sign"] }, ["verify"], null],
              ["hmac-verify-ok", "HMAC", { key_ops: ["verify"] }, ["verify"], ["verify"]],
              ["hmac-verify-missing", "HMAC", { key_ops: ["verify"] }, ["sign"], null],
              ["hmac-use-sig-all", "HMAC", { use: "sig" }, ["sign", "verify"], ["sign", "verify"]],
              ["hmac-use-sig-subset", "HMAC", { use: "sig" }, ["sign"], ["sign"]],
              ["aes-unknown-quoted", "AES-CBC", { key_ops: ["'encrypt'", "decrypt"] }, ["decrypt"], ["decrypt"]],
              ["aes-unknown-space-foo", "AES-CBC", { key_ops: ["encrypt ", "foo", "decrypt"] }, ["decrypt"], ["decrypt"]],
              ["aes-unknown-case", "AES-CBC", { key_ops: ["Encrypt", "decrypt"] }, ["decrypt"], ["decrypt"]],
              ["aes-unknown-quoted-missing", "AES-CBC", { key_ops: ["'encrypt'", "decrypt"] }, ["encrypt"], null],
              ["aes-unknown-space-missing", "AES-CBC", { key_ops: ["encrypt "] }, ["encrypt"], null],
              ["aes-unknown-case-missing", "AES-CBC", { key_ops: ["Encrypt"] }, ["encrypt"], null]
            ];
            for (const [label, family, jwkUsages, importUsages, expectedUsages] of cases) {
              await expectImport(family, label, jwkUsages, importUsages, expectedUsages);
            }
            globalThis.__cryptoJwkImportUseValueFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle JWK import use-value matrix should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoJwkImportUseValueFailures)")
        .expect("crypto subtle JWK import use-value matrix promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_jwk_import_edges_match_chromium_legacy() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoJwkImportEdgeFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );

            // Chromium legacy test: crypto/subtle/aes-cbc/import-jwk.html.
            // The upstream fixture also encrypts with AES-CBC. Moli does
            // not ship an AES primitive in this branch, so this pins the
            // observable import/key-shape half without pretending encryption
            // is supported.
            const aesCbcJwk = {
              kty: "oct",
              alg: "A256CBC",
              use: "enc",
              ext: true,
              k: "YD3rEBXKcb4rc67whX13gR81LAc7YQjXLZgQowkU3_Q"
            };
            const aesCbcKey = await subtle.importKey(
              "jwk",
              aesCbcJwk,
              { name: "AES-CBC" },
              false,
              ["encrypt"]
            );
            if (
              aesCbcKey.type !== "secret" ||
              aesCbcKey.extractable !== false ||
              aesCbcKey.algorithm.name !== "AES-CBC" ||
              aesCbcKey.algorithm.length !== 256 ||
              aesCbcKey.usages.join(",") !== "encrypt"
            ) {
              failures.push([
                "aes-cbc-import-jwk",
                aesCbcKey.type,
                aesCbcKey.extractable,
                aesCbcKey.algorithm.name,
                aesCbcKey.algorithm.length,
                aesCbcKey.usages.join("+")
              ].join(":"));
            }

            // Chromium legacy test: crypto/subtle/import-jwk.html.
            const hmac256 = { name: "HMAC", hash: { name: "sha-256" } };
            const validOctKey = "ahjkn23387fgnsibf23qsvahjkn37387fgnsibf23qs";
            const expectedErrors = [
              ["null-aes", subtle.importKey("jwk", null, { name: "aes-cbc" }, true, ["encrypt"]), "DataError"],
              ["undefined-aes", subtle.importKey("jwk", undefined, { name: "aes-cbc" }, true, ["encrypt"]), "DataError"],
              ["empty-aes", subtle.importKey("jwk", {}, { name: "aes-cbc" }, true, ["encrypt"]), "DataError"],
              [
                "unknown-kty",
                subtle.importKey(
                  "jwk",
                  { kty: "foobar", alg: "HS256", use: "sig", k: validOctKey },
                  hmac256,
                  true,
                  ["sign"]
                ),
                "DataError"
              ],
              [
                "unknown-aes-alg",
                subtle.importKey(
                  "jwk",
                  { kty: "oct", alg: "foobar", use: "enc", k: validOctKey },
                  { name: "aes-cbc" },
                  true,
                  ["encrypt"]
                ),
                "DataError"
              ],
              [
                "aes-alg-mismatch",
                subtle.importKey(
                  "jwk",
                  { kty: "oct", alg: "HS256", use: "enc", ext: false, k: validOctKey },
                  { name: "AES-cbc" },
                  false,
                  ["encrypt"]
                ),
                "DataError"
              ],
              [
                "hmac-alg-mismatch",
                subtle.importKey(
                  "jwk",
                  { kty: "oct", alg: "HS256", use: "sig", ext: false, k: validOctKey },
                  { name: "hmac", hash: { name: "sha-1" } },
                  false,
                  ["sign"]
                ),
                "DataError"
              ],
              [
                "missing-hmac-k",
                subtle.importKey("jwk", { kty: "oct", alg: "HS256" }, hmac256, true, ["sign"]),
                "DataError"
              ],
              [
                "missing-aes-k",
                subtle.importKey("jwk", { kty: "oct", alg: "A128CBC" }, { name: "aes-cbc" }, true, ["encrypt"]),
                "DataError"
              ],
              [
                "short-aes-k",
                subtle.importKey(
                  "jwk",
                  { kty: "oct", alg: "A128CBC", use: "enc", ext: false, k: "1234" },
                  { name: "aes-cbc" },
                  false,
                  ["encrypt"]
                ),
                "DataError"
              ],
              [
                "aes-k-alg-length-mismatch",
                subtle.importKey(
                  "jwk",
                  { kty: "oct", alg: "A128CBC", use: "enc", ext: false, k: validOctKey },
                  { name: "aes-cbc" },
                  false,
                  ["encrypt"]
                ),
                "DataError"
              ],
              [
                "hmac-k-non-url-base64",
                subtle.importKey(
                  "jwk",
                  { kty: "oct", alg: "HS256", use: "sig", ext: false, k: "ahjkn23387f+nsibf23qsvahjkn37387fgnsibf23qs" },
                  hmac256,
                  false,
                  ["sign"]
                ),
                "DataError"
              ],
              [
                "coerced-kty",
                subtle.importKey(
                  "jwk",
                  { kty: 1, alg: "HS256", use: "sig", ext: false, k: validOctKey },
                  hmac256,
                  false,
                  ["sign"]
                ),
                "DataError"
              ],
              [
                "coerced-alg",
                subtle.importKey(
                  "jwk",
                  { kty: "oct", alg: 1, use: "enc", ext: false, k: validOctKey },
                  { name: "aes-cbc" },
                  false,
                  ["encrypt"]
                ),
                "DataError"
              ],
              [
                "coerced-use",
                subtle.importKey(
                  "jwk",
                  { kty: "oct", alg: "HS256", use: 1, ext: false, k: validOctKey },
                  hmac256,
                  false,
                  ["sign"]
                ),
                "DataError"
              ]
            ];
            for (const [label, promise, expected] of expectedErrors) {
              const actual = await rejectionName(promise);
              if (actual !== expected) {
                failures.push(`${label}:${actual}`);
              }
            }

            const extStringKey = await subtle.importKey(
              "jwk",
              { kty: "oct", alg: "HS256", use: "sig", ext: "false", k: validOctKey },
              hmac256,
              false,
              ["sign"]
            );
            if (
              extStringKey.type !== "secret" ||
              extStringKey.algorithm.name !== "HMAC" ||
              extStringKey.usages.join(",") !== "sign"
            ) {
              failures.push("ext-string-success");
            }

            const numericKKey = await subtle.importKey(
              "jwk",
              { kty: "oct", alg: "HS256", use: "sig", ext: false, k: 1258 },
              hmac256,
              false,
              ["sign"]
            );
            if (
              numericKKey.type !== "secret" ||
              numericKKey.algorithm.name !== "HMAC" ||
              numericKKey.usages.join(",") !== "sign"
            ) {
              failures.push("numeric-k-success");
            }

            globalThis.__cryptoJwkImportEdgeFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle JWK import edge probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoJwkImportEdgeFailures)")
        .expect("crypto subtle JWK import edge promise chain should settle");

    assert_eq!(result, "[]");
}
