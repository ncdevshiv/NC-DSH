use super::*;

#[test]
fn crypto_key_structured_clone_preserves_x25519_key_material() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoX25519CloneFailures = ["pending"];
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
            // Chromium WPT: WebCryptoAPI/derive_bits_keys/cfrg_curves_bits_fixtures.js.
            // Chromium legacy cloneKey tests cover asymmetric keys for NIST EC
            // and RSA; X25519 is the asymmetric key family backed in
            // Moli's current WebCrypto scope, so clone coverage uses the
            // CFRG vector here.
            const privatePkcs8 = new Uint8Array([
              48, 46, 2, 1, 0, 48, 5, 6, 3, 43, 101, 110, 4, 34, 4, 32,
              200, 131, 142, 118, 208, 87, 223, 183, 216, 201, 90, 105,
              225, 56, 22, 10, 221, 99, 115, 253, 113, 164, 210, 118,
              187, 86, 227, 168, 27, 100, 255, 97
            ]);
            const publicSpki = new Uint8Array([
              48, 42, 48, 5, 6, 3, 43, 101, 110, 3, 33, 0, 28, 242,
              177, 230, 2, 46, 197, 55, 55, 30, 215, 245, 62, 84, 250,
              17, 84, 216, 62, 152, 235, 100, 234, 81, 250, 229, 179,
              48, 124, 254, 151, 6
            ]);
            const publicRaw = publicSpki.slice(12);
            const expectedBits = new Uint8Array([
              39, 104, 64, 157, 250, 185, 158, 194, 59, 140, 137, 185,
              63, 245, 136, 2, 149, 247, 97, 118, 8, 143, 137, 228,
              61, 254, 190, 126, 161, 149, 0, 8
            ]);
            const privateKey = await subtle.importKey(
              "pkcs8",
              privatePkcs8,
              "X25519",
              false,
              ["deriveBits"]
            );
            const publicKey = await subtle.importKey(
              "spki",
              publicSpki,
              "X25519",
              true,
              []
            );
            privateKey.extraProperty = "source-only";
            publicKey.extraProperty = "source-only";

            // Mutate the script-visible wrapper objects before cloning;
            // structured clone must serialize immutable internal slots.
            privateKey.algorithm.name = "HMAC";
            privateKey.usages.push("deriveKey");
            publicKey.algorithm.name = "HMAC";

            const privateClone = structuredClone(privateKey);
            const publicClone = structuredClone(publicKey);
            const derived = await subtle.deriveBits(
              { name: "X25519", public: publicClone },
              privateClone,
              256
            );

            if (privateClone === privateKey || publicClone === publicKey) {
              failures.push("identity");
            }
            if (privateClone.extraProperty !== undefined || publicClone.extraProperty !== undefined) {
              failures.push("expando");
            }
            if (
              !(privateClone instanceof CryptoKey) ||
              privateClone.type !== "private" ||
              privateClone.extractable !== false ||
              privateClone.algorithm.name !== "X25519" ||
              privateClone.usages.join(",") !== "deriveBits"
            ) {
              failures.push("private-shape");
            }
            if (
              !(publicClone instanceof CryptoKey) ||
              publicClone.type !== "public" ||
              publicClone.extractable !== true ||
              publicClone.algorithm.name !== "X25519" ||
              publicClone.usages.join(",") !== ""
            ) {
              failures.push("public-shape");
            }
            if (!sameBytes(derived, expectedBits)) {
              failures.push("deriveBits");
            }
            if (await rejectionName(subtle.exportKey("pkcs8", privateClone)) !== "InvalidAccessError") {
              failures.push("private-export");
            }
            if (!sameBytes(await subtle.exportKey("raw", publicClone), publicRaw)) {
              failures.push("public-export");
            }

            globalThis.__cryptoX25519CloneFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto key X25519 structured clone probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoX25519CloneFailures)")
        .expect("crypto key X25519 structured clone promise should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_x25519_import_export_supports_wpt_key_formats() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoX25519FormatProbe = [];
          (async () => {
            const subtle = crypto.subtle;
            const sameBytes = (left, right) => {
              const a = new Uint8Array(left);
              const b = new Uint8Array(right);
              return a.length === b.length && a.every((value, index) => value === b[index]);
            };
            // Chromium WPT: WebCryptoAPI/import_export/okp_importKey_fixtures.js
            const publicRaw = new Uint8Array([
              28, 242, 177, 230, 2, 46, 197, 55, 55, 30, 215, 245, 62, 84, 250, 17,
              84, 216, 62, 152, 235, 100, 234, 81, 250, 229, 179, 48, 124, 254, 151, 6
            ]);
            const spki = new Uint8Array([
              48, 42, 48, 5, 6, 3, 43, 101, 110, 3, 33, 0, 28, 242, 177, 230,
              2, 46, 197, 55, 55, 30, 215, 245, 62, 84, 250, 17, 84, 216, 62,
              152, 235, 100, 234, 81, 250, 229, 179, 48, 124, 254, 151, 6
            ]);
            const pkcs8 = new Uint8Array([
              48, 46, 2, 1, 0, 48, 5, 6, 3, 43, 101, 110, 4, 34, 4, 32,
              200, 131, 142, 118, 208, 87, 223, 183, 216, 201, 90, 105, 225,
              56, 22, 10, 221, 99, 115, 253, 113, 164, 210, 118, 187, 86,
              227, 168, 27, 100, 255, 97
            ]);
            const jwkPrivate = {
              kty: "OKP",
              crv: "X25519",
              x: "HPKx5gIuxTc3Htf1PlT6EVTYPpjrZOpR-uWzMHz-lwY",
              d: "yIOOdtBX37fYyVpp4TgWCt1jc_1xpNJ2u1bjqBtk_2E",
              alg: "this is ignored"
            };
            const jwkPublic = {
              kty: "OKP",
              crv: "X25519",
              x: "HPKx5gIuxTc3Htf1PlT6EVTYPpjrZOpR-uWzMHz-lwY",
              alg: "this is ignored"
            };

            const rawPublicKey = await subtle.importKey("raw", publicRaw, "X25519", true, []);
            const spkiPublicKey = await subtle.importKey("spki", spki, { name: "X25519" }, true, []);
            const pkcs8PrivateKey = await subtle.importKey(
              "pkcs8",
              pkcs8,
              "X25519",
              true,
              ["deriveBits"]
            );
            const jwkPrivateKey = await subtle.importKey(
              "jwk",
              jwkPrivate,
              { name: "X25519" },
              true,
              ["deriveBits"]
            );
            const jwkPublicKey = await subtle.importKey("jwk", jwkPublic, "X25519", true, []);

            const rawRoundTrip = await subtle.exportKey("raw", rawPublicKey);
            const spkiRoundTrip = await subtle.exportKey("spki", spkiPublicKey);
            const pkcs8RoundTrip = await subtle.exportKey("pkcs8", pkcs8PrivateKey);
            const privateJwkRoundTrip = await subtle.exportKey("jwk", jwkPrivateKey);
            const publicJwkRoundTrip = await subtle.exportKey("jwk", jwkPublicKey);
            const shared = await subtle.deriveBits(
              { name: "X25519", public: rawPublicKey },
              pkcs8PrivateKey,
              128
            );

            globalThis.__cryptoX25519FormatProbe = [
              rawPublicKey.type,
              rawPublicKey.algorithm.name,
              rawPublicKey.usages.join(","),
              String(sameBytes(publicRaw, rawRoundTrip)),
              spkiPublicKey.type,
              String(sameBytes(spki, spkiRoundTrip)),
              pkcs8PrivateKey.type,
              pkcs8PrivateKey.usages.join(","),
              String(sameBytes(pkcs8, pkcs8RoundTrip)),
              privateJwkRoundTrip.kty,
              privateJwkRoundTrip.crv,
              privateJwkRoundTrip.x,
              privateJwkRoundTrip.d,
              String("alg" in privateJwkRoundTrip),
              privateJwkRoundTrip.key_ops.join(","),
              String(privateJwkRoundTrip.ext),
              publicJwkRoundTrip.kty,
              publicJwkRoundTrip.crv,
              String("d" in publicJwkRoundTrip),
              String("alg" in publicJwkRoundTrip),
              String(shared.byteLength)
            ];
          })();
        })()
        "#,
    )
    .expect("crypto subtle X25519 import/export probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoX25519FormatProbe)")
        .expect("crypto subtle X25519 import/export promise chain should settle");

    assert_eq!(
        result,
        r#"["public","X25519","","true","public","true","private","deriveBits","true","OKP","X25519","HPKx5gIuxTc3Htf1PlT6EVTYPpjrZOpR-uWzMHz-lwY","yIOOdtBX37fYyVpp4TgWCt1jc_1xpNJ2u1bjqBtk_2E","false","deriveBits","true","OKP","X25519","false","false","16"]"#
    );
}
#[test]
fn crypto_subtle_get_public_key_supports_x25519_private_keys() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoGetPublicKeyProbe = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const sameBytes = (left, right) => {
              const a = new Uint8Array(left);
              const b = new Uint8Array(right);
              return a.length === b.length && a.every((value, index) => value === b[index]);
            };
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );

            // Chromium WPT: WebCryptoAPI/getPublicKey.tentative.https.any.js.
            const generated = await subtle.generateKey(
              { name: "X25519" },
              false,
              ["deriveBits", "deriveKey"]
            );
            const derivedGeneratedPublic = await subtle.getPublicKey(
              generated.privateKey,
              []
            );
            const originalGeneratedSpki = await subtle.exportKey(
              "spki",
              generated.publicKey
            );
            const derivedGeneratedSpki = await subtle.exportKey(
              "spki",
              derivedGeneratedPublic
            );

            // Chromium WPT: WebCryptoAPI/import_export/okp_importKey_fixtures.js.
            const publicRaw = new Uint8Array([
              28, 242, 177, 230, 2, 46, 197, 55, 55, 30, 215, 245, 62, 84, 250, 17,
              84, 216, 62, 152, 235, 100, 234, 81, 250, 229, 179, 48, 124, 254, 151, 6
            ]);
            const spki = new Uint8Array([
              48, 42, 48, 5, 6, 3, 43, 101, 110, 3, 33, 0, 28, 242, 177, 230,
              2, 46, 197, 55, 55, 30, 215, 245, 62, 84, 250, 17, 84, 216, 62,
              152, 235, 100, 234, 81, 250, 229, 179, 48, 124, 254, 151, 6
            ]);
            const pkcs8 = new Uint8Array([
              48, 46, 2, 1, 0, 48, 5, 6, 3, 43, 101, 110, 4, 34, 4, 32,
              200, 131, 142, 118, 208, 87, 223, 183, 216, 201, 90, 105, 225,
              56, 22, 10, 221, 99, 115, 253, 113, 164, 210, 118, 187, 86,
              227, 168, 27, 100, 255, 97
            ]);
            const importedPrivate = await subtle.importKey(
              "pkcs8",
              pkcs8,
              "X25519",
              false,
              ["deriveBits"]
            );
            const importedPublic = await subtle.getPublicKey(importedPrivate, []);
            const importedRaw = await subtle.exportKey("raw", importedPublic);
            const importedSpki = await subtle.exportKey("spki", importedPublic);

            const aesKey = await subtle.generateKey(
              { name: "AES-GCM", length: 128 },
              true,
              ["encrypt"]
            );
            const hmacKey = await subtle.generateKey(
              { name: "HMAC", hash: "SHA-256" },
              true,
              ["sign"]
            );
            const errors = await Promise.all([
              rejectionName(subtle.getPublicKey(generated.publicKey, [])),
              rejectionName(subtle.getPublicKey(generated.privateKey, ["deriveBits"])),
              rejectionName(subtle.getPublicKey(aesKey, [])),
              rejectionName(subtle.getPublicKey(hmacKey, [])),
              rejectionName(subtle.getPublicKey(importedPrivate))
            ]);

            globalThis.__cryptoGetPublicKeyProbe = [
              String("getPublicKey" in subtle),
              typeof subtle.getPublicKey,
              derivedGeneratedPublic.type,
              derivedGeneratedPublic.algorithm.name,
              String(derivedGeneratedPublic.extractable),
              derivedGeneratedPublic.usages.join(","),
              String(sameBytes(originalGeneratedSpki, derivedGeneratedSpki)),
              importedPublic.type,
              importedPublic.algorithm.name,
              String(importedPublic.extractable),
              importedPublic.usages.join(","),
              String(sameBytes(publicRaw, importedRaw)),
              String(sameBytes(spki, importedSpki)),
              errors.join(",")
            ];
          })();
        })()
        "#,
    )
    .expect("crypto subtle getPublicKey probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoGetPublicKeyProbe)")
        .expect("crypto subtle getPublicKey promise chain should settle");

    assert_eq!(
        result,
        r#"["true","function","public","X25519","true","","true","public","X25519","true","","true","true","InvalidAccessError,SyntaxError,NotSupportedError,NotSupportedError,TypeError"]"#
    );
}
#[test]
fn crypto_subtle_x25519_derive_bits_and_key_match_chromium_wpt_vectors() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoX25519WptDeriveFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const sameBytes = (left, right) => {
              const a = new Uint8Array(left);
              const b = new Uint8Array(right);
              return a.length === b.length && a.every((value, index) => value === b[index]);
            };
            const sameLeadingBits = (actual, expected, bitCount) => {
              const a = new Uint8Array(actual);
              const b = new Uint8Array(expected);
              const fullBytes = Math.floor(bitCount / 8);
              for (let index = 0; index < fullBytes; index += 1) {
                if (a[index] !== b[index]) return false;
              }
              const remainder = bitCount % 8;
              return remainder === 0 || (a[fullBytes] >> (8 - remainder)) === (b[fullBytes] >> (8 - remainder));
            };
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );

            // Chromium WPT: WebCryptoAPI/derive_bits_keys/cfrg_curves_bits_fixtures.js
            const pkcs8 = new Uint8Array([
              48, 46, 2, 1, 0, 48, 5, 6, 3, 43, 101, 110, 4, 34, 4, 32,
              200, 131, 142, 118, 208, 87, 223, 183, 216, 201, 90, 105, 225,
              56, 22, 10, 221, 99, 115, 253, 113, 164, 210, 118, 187, 86,
              227, 168, 27, 100, 255, 97
            ]);
            const spki = new Uint8Array([
              48, 42, 48, 5, 6, 3, 43, 101, 110, 3, 33, 0, 28, 242, 177, 230,
              2, 46, 197, 55, 55, 30, 215, 245, 62, 84, 250, 17, 84, 216, 62,
              152, 235, 100, 234, 81, 250, 229, 179, 48, 124, 254, 151, 6
            ]);
            const expected = new Uint8Array([
              39, 104, 64, 157, 250, 185, 158, 194, 59, 140, 137, 185, 63, 245,
              136, 2, 149, 247, 97, 118, 8, 143, 137, 228, 61, 254, 190, 126,
              161, 149, 0, 8
            ]);
            const expected230 = new Uint8Array([
              39, 104, 64, 157, 250, 185, 158, 194, 59, 140, 137, 185, 63, 245,
              136, 2, 149, 247, 97, 118, 8, 143, 137, 228, 61, 254, 190, 126,
              160
            ]);
            const smallOrderSpki = new Uint8Array([
              48, 42, 48, 5, 6, 3, 43, 101, 110, 3, 33, 0, 0, 0, 0, 0, 0, 0,
              0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
              0, 0, 0, 0, 0
            ]);

            const privateKey = await subtle.importKey("pkcs8", pkcs8, { name: "X25519" }, true, ["deriveBits", "deriveKey"]);
            const publicKey = await subtle.importKey("spki", spki, { name: "X25519" }, true, []);
            const noDeriveBitsKey = await subtle.importKey("pkcs8", pkcs8, "X25519", false, ["deriveKey"]);
            const noDeriveKeyKey = await subtle.importKey("pkcs8", pkcs8, "X25519", false, ["deriveBits"]);

            const fullBits = await subtle.deriveBits({ name: "X25519", public: publicKey }, privateKey, 256);
            if (!sameBytes(fullBits, expected)) failures.push("deriveBits:full");
            const omittedBits = await subtle.deriveBits({ name: "X25519", public: publicKey }, privateKey);
            if (!sameBytes(omittedBits, expected)) failures.push("deriveBits:omitted");
            const nullBits = await subtle.deriveBits({ name: "X25519", public: publicKey }, privateKey, null);
            if (!sameBytes(nullBits, expected)) failures.push("deriveBits:null");
            const undefinedBits = await subtle.deriveBits({ name: "X25519", public: publicKey }, privateKey, undefined);
            if (!sameBytes(undefinedBits, expected)) failures.push("deriveBits:undefined");
            const zeroBits = await subtle.deriveBits({ name: "X25519", public: publicKey }, privateKey, 0);
            if (new Uint8Array(zeroBits).byteLength !== 0) failures.push("deriveBits:zero-length");
            const shortBits = await subtle.deriveBits({ name: "x25519", public: publicKey }, privateKey, 224);
            if (!sameLeadingBits(shortBits, expected, 224) || new Uint8Array(shortBits).byteLength !== 28) {
              failures.push("deriveBits:short");
            }
            const bits230 = await subtle.deriveBits({ name: "X25519", public: publicKey }, privateKey, 230);
            if (!sameBytes(bits230, expected230)) failures.push("deriveBits:230");
            const oddBits = await subtle.deriveBits({ name: "X25519", public: publicKey }, privateKey, 245);
            if (!sameLeadingBits(oddBits, expected, 245) || new Uint8Array(oddBits).byteLength !== 31) {
              failures.push("deriveBits:non-multiple");
            }

            const derivedKey = await subtle.deriveKey(
              { name: "X25519", public: publicKey },
              privateKey,
              { name: "HMAC", hash: "SHA-256", length: 256 },
              true,
              ["sign", "verify"]
            );
            if (
              derivedKey.type !== "secret" ||
              derivedKey.algorithm.name !== "HMAC" ||
              derivedKey.algorithm.hash.name !== "SHA-256" ||
              derivedKey.algorithm.length !== 256 ||
              derivedKey.usages.join(",") !== "sign,verify" ||
              !sameBytes(await subtle.exportKey("raw", derivedKey), expected)
            ) {
              failures.push("deriveKey:hmac-shape");
            }
            const signOnlyDerived = await subtle.deriveKey(
              { name: "x25519", public: publicKey },
              privateKey,
              { name: "HMAC", hash: "SHA-256", length: 128 },
              true,
              ["sign"]
            );
            if (
              signOnlyDerived.usages.join(",") !== "sign" ||
              new Uint8Array(await subtle.exportKey("raw", signOnlyDerived)).byteLength !== 16
            ) {
              failures.push("deriveKey:short-hmac");
            }
            // Chromium backend test:
            // components/webcrypto/algorithms/ecdh_unittest.cc
            // DeriveKeyHmac19Bits. X25519 exercises the same direct key
            // agreement path in Moli's no-new-deps implementation.
            const nineteenBitHmac = await subtle.deriveKey(
              { name: "X25519", public: publicKey },
              privateKey,
              { name: "HMAC", hash: "SHA-1", length: 19 },
              true,
              ["sign"]
            );
            const nineteenBitRaw = new Uint8Array(await subtle.exportKey("raw", nineteenBitHmac));
            if (
              nineteenBitHmac.algorithm.length !== 19 ||
              nineteenBitRaw.byteLength !== 3 ||
              !sameLeadingBits(nineteenBitRaw, expected, 19) ||
              (nineteenBitRaw[2] & 0x1f) !== 0
            ) {
              failures.push("deriveKey:hmac-19-bits");
            }
            // Chromium legacy test crypto/subtle/derive-hkdf-keys.html derives
            // an HKDF key from ECDH. Moli does not implement NIST ECDH
            // in its current WebCrypto scope, so X25519 exercises the same
            // deriveKey-to-KDF contract with the supported 256-bit agreement.
            const hkdfParams = {
              name: "HKDF",
              hash: "SHA-256",
              salt: new Uint8Array([9, 8, 7]),
              info: new Uint8Array([1, 2, 3])
            };
            const derivedHkdfKey = await subtle.deriveKey(
              { name: "X25519", public: publicKey },
              privateKey,
              "HKDF",
              false,
              ["deriveBits", "deriveKey"]
            );
            const expectedHkdfKey = await subtle.importKey("raw", expected, "HKDF", false, ["deriveBits"]);
            if (
              derivedHkdfKey.type !== "secret" ||
              derivedHkdfKey.extractable !== false ||
              derivedHkdfKey.algorithm.name !== "HKDF" ||
              derivedHkdfKey.usages.join(",") !== "deriveKey,deriveBits" ||
              !sameBytes(
                await subtle.deriveBits(hkdfParams, derivedHkdfKey, 128),
                await subtle.deriveBits(hkdfParams, expectedHkdfKey, 128)
              )
            ) {
              failures.push("deriveKey:hkdf");
            }
            const pbkdf2Params = {
              name: "PBKDF2",
              hash: "SHA-256",
              salt: new Uint8Array([6, 5, 4]),
              iterations: 2
            };
            const derivedPbkdf2Key = await subtle.deriveKey(
              { name: "X25519", public: publicKey },
              privateKey,
              "PBKDF2",
              false,
              ["deriveBits"]
            );
            const expectedPbkdf2Key = await subtle.importKey("raw", expected, "PBKDF2", false, ["deriveBits"]);
            if (
              derivedPbkdf2Key.type !== "secret" ||
              derivedPbkdf2Key.extractable !== false ||
              derivedPbkdf2Key.algorithm.name !== "PBKDF2" ||
              derivedPbkdf2Key.usages.join(",") !== "deriveBits" ||
              !sameBytes(
                await subtle.deriveBits(pbkdf2Params, derivedPbkdf2Key, 128),
                await subtle.deriveBits(pbkdf2Params, expectedPbkdf2Key, 128)
              )
            ) {
              failures.push("deriveKey:pbkdf2");
            }

            const smallOrderPublicKey = await subtle.importKey("spki", smallOrderSpki, "X25519", true, []);
            const errors = await Promise.all([
              rejectionName(subtle.deriveBits({ name: "X25519", public: publicKey }, noDeriveBitsKey, 256)),
              rejectionName(subtle.deriveKey(
                { name: "X25519", public: publicKey },
                noDeriveKeyKey,
                { name: "HMAC", hash: "SHA-256", length: 256 },
                true,
                ["sign"]
              )),
              rejectionName(subtle.deriveBits({ name: "X25519" }, privateKey, 256)),
              rejectionName(subtle.deriveKey(
                { name: "X25519", public: privateKey },
                privateKey,
                { name: "HMAC", hash: "SHA-256", length: 256 },
                true,
                ["sign"]
              )),
              rejectionName(subtle.deriveBits({ name: "X25519", public: publicKey }, privateKey, 264)),
              rejectionName(subtle.deriveBits({ name: "X25519", public: smallOrderPublicKey }, privateKey, 256)),
              rejectionName(subtle.deriveKey(
                { name: "X25519", public: publicKey },
                privateKey,
                { name: "HMAC", hash: "SHA-256", length: 512 },
                true,
                ["sign"]
              )),
              rejectionName(subtle.deriveKey(
                { name: "X25519", public: publicKey },
                privateKey,
                { name: "HMAC", hash: "SHA-256" },
                true,
                ["sign"]
              )),
              rejectionName(subtle.deriveKey(
                { name: "X25519", public: publicKey },
                privateKey,
                "HKDF",
                true,
                ["deriveBits"]
              ))
            ]);
            const expectedErrors = [
              "InvalidAccessError",
              "InvalidAccessError",
              "TypeError",
              "InvalidAccessError",
              "OperationError",
              "OperationError",
              "OperationError",
              "OperationError",
              "SyntaxError"
            ];
            if (errors.join(",") !== expectedErrors.join(",")) {
              failures.push("derive-errors:" + errors.join(","));
            }
            globalThis.__cryptoX25519WptDeriveFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle X25519 WPT derive probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoX25519WptDeriveFailures)")
        .expect("crypto subtle X25519 WPT derive promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_x25519_generate_and_import_failures_match_wpt_boundaries() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoX25519WptImportFailures = ["pending"];
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
              return a.length === b.length && a.every((value, index) => value === b[index]);
            };
            const publicRaw = new Uint8Array([
              28, 242, 177, 230, 2, 46, 197, 55, 55, 30, 215, 245, 62, 84, 250, 17,
              84, 216, 62, 152, 235, 100, 234, 81, 250, 229, 179, 48, 124, 254, 151, 6
            ]);
            const spki = new Uint8Array([
              48, 42, 48, 5, 6, 3, 43, 101, 110, 3, 33, 0, 28, 242, 177, 230,
              2, 46, 197, 55, 55, 30, 215, 245, 62, 84, 250, 17, 84, 216, 62,
              152, 235, 100, 234, 81, 250, 229, 179, 48, 124, 254, 151, 6
            ]);
            const pkcs8 = new Uint8Array([
              48, 46, 2, 1, 0, 48, 5, 6, 3, 43, 101, 110, 4, 34, 4, 32,
              200, 131, 142, 118, 208, 87, 223, 183, 216, 201, 90, 105, 225,
              56, 22, 10, 221, 99, 115, 253, 113, 164, 210, 118, 187, 86,
              227, 168, 27, 100, 255, 97
            ]);
            const jwkPrivate = {
              kty: "OKP",
              crv: "X25519",
              x: "HPKx5gIuxTc3Htf1PlT6EVTYPpjrZOpR-uWzMHz-lwY",
              d: "yIOOdtBX37fYyVpp4TgWCt1jc_1xpNJ2u1bjqBtk_2E"
            };
            const jwkPublic = {
              kty: "OKP",
              crv: "X25519",
              x: "HPKx5gIuxTc3Htf1PlT6EVTYPpjrZOpR-uWzMHz-lwY"
            };
            const sameJwkMembers = (expected, actual) =>
              Object.keys(expected).every((name) => actual[name] === expected[name]);
            const expectedX25519PrivateUsages = (usages) =>
              ["deriveKey", "deriveBits"].filter((usage) => usages.includes(usage)).join(",");
            const assertX25519JwkExport = (label, expected, actual, expectedUsages) => {
              if (
                !sameJwkMembers(expected, actual) ||
                actual.kty !== "OKP" ||
                actual.crv !== "X25519" ||
                "alg" in actual ||
                actual.key_ops.join(",") !== expectedUsages ||
                actual.ext !== true
              ) {
                failures.push(label + ":jwk-export");
              }
            };

            const stringAlgorithmPair = await subtle.generateKey("X25519", true, ["deriveBits"]);
            if (
              stringAlgorithmPair.privateKey.algorithm.name !== "X25519" ||
              stringAlgorithmPair.publicKey.algorithm.name !== "X25519" ||
              stringAlgorithmPair.privateKey.usages.join(",") !== "deriveBits" ||
              stringAlgorithmPair.publicKey.usages.join(",") !== ""
            ) {
              failures.push("generate-string-algorithm-shape");
            }
            const duplicateUsagePair = await subtle.generateKey(
              "X25519",
              true,
              ["deriveBits", "deriveKey", "deriveBits"]
            );
            if (
              duplicateUsagePair.privateKey.usages.join(",") !== "deriveKey,deriveBits" ||
              duplicateUsagePair.publicKey.usages.join(",") !== ""
            ) {
              failures.push("generate-duplicate-usages");
            }

            for (const usages of [["deriveBits"], ["deriveKey"], ["deriveBits", "deriveKey"]]) {
              for (const extractable of [true, false]) {
                const pair = await subtle.generateKey({ name: "X25519" }, extractable, usages);
                const expectedPrivateUsages = usages.includes("deriveKey") && usages.includes("deriveBits")
                  ? "deriveKey,deriveBits"
                  : usages.join(",");
                if (
                  pair.privateKey.type !== "private" ||
                  pair.publicKey.type !== "public" ||
                  pair.privateKey.extractable !== extractable ||
                  pair.publicKey.extractable !== true ||
                  pair.privateKey.usages.join(",") !== expectedPrivateUsages ||
                  pair.publicKey.usages.join(",") !== ""
                ) {
                  failures.push("generate-shape:" + usages.join("+") + ":" + extractable);
                }
                if (new Uint8Array(await subtle.exportKey("raw", pair.publicKey)).byteLength !== 32) {
                  failures.push("generate-public-raw:" + usages.join("+"));
                }
                if (extractable) {
                  const exportedPrivate = await subtle.exportKey("jwk", pair.privateKey);
                  if (
                    exportedPrivate.kty !== "OKP" ||
                    exportedPrivate.crv !== "X25519" ||
                    typeof exportedPrivate.x !== "string" ||
                    typeof exportedPrivate.d !== "string" ||
                    exportedPrivate.key_ops.join(",") !== expectedPrivateUsages
                  ) {
                    failures.push("generate-private-jwk:" + usages.join("+"));
                  }
                }
              }
            }

            // Chromium WPT: WebCryptoAPI/import_export/okp_importKey_X25519.https.any.js.
            // This compact matrix covers every X25519 OKP format family,
            // algorithm identifier shape, extractability value, and the
            // private usage subsets generated by allValidUsages().
            const algorithmInputs = ["X25519", { name: "X25519" }];
            const publicImportCases = [
              ["raw", publicRaw],
              ["spki", spki],
              ["jwk", jwkPublic]
            ];
            const privateImportCases = [
              ["pkcs8", pkcs8],
              ["jwk", jwkPrivate]
            ];
            const privateUsageSets = [
              ["deriveKey"],
              ["deriveBits"],
              ["deriveBits", "deriveKey"],
              ["deriveKey", "deriveBits", "deriveKey", "deriveBits"]
            ];
            for (const algorithm of algorithmInputs) {
              const algorithmLabel = typeof algorithm === "string" ? algorithm : "object";
              for (const extractable of [true, false]) {
                for (const [format, keyData] of publicImportCases) {
                  const key = await subtle.importKey(format, keyData, algorithm, extractable, []);
                  if (
                    !(key instanceof CryptoKey) ||
                    key.type !== "public" ||
                    key.algorithm.name !== "X25519" ||
                    key.extractable !== extractable ||
                    key.usages.join(",") !== ""
                  ) {
                    failures.push("public-matrix-shape:" + algorithmLabel + ":" + format + ":" + extractable);
                  }
                  if (extractable) {
                    const exported = await subtle.exportKey(format, key);
                    if (format === "jwk") {
                      assertX25519JwkExport("public-matrix:" + algorithmLabel + ":" + format, jwkPublic, exported, "");
                    } else if (!sameBytes(exported, keyData)) {
                      failures.push("public-matrix-roundtrip:" + algorithmLabel + ":" + format);
                    }
                  } else {
                    const error = await rejectionName(subtle.exportKey(format, key));
                    if (error !== "InvalidAccessError") {
                      failures.push("public-matrix-unextractable:" + algorithmLabel + ":" + format + ":" + error);
                    }
                  }
                }

                for (const [format, keyData] of privateImportCases) {
                  for (const usages of privateUsageSets) {
                    const key = await subtle.importKey(format, keyData, algorithm, extractable, usages);
                    const expectedUsages = expectedX25519PrivateUsages(usages);
                    if (
                      !(key instanceof CryptoKey) ||
                      key.type !== "private" ||
                      key.algorithm.name !== "X25519" ||
                      key.extractable !== extractable ||
                      key.usages.join(",") !== expectedUsages
                    ) {
                      failures.push("private-matrix-shape:" + algorithmLabel + ":" + format + ":" + extractable + ":" + usages.join("+"));
                    }
                    if (extractable) {
                      const exported = await subtle.exportKey(format, key);
                      if (format === "jwk") {
                        assertX25519JwkExport(
                          "private-matrix:" + algorithmLabel + ":" + format + ":" + usages.join("+"),
                          jwkPrivate,
                          exported,
                          expectedUsages
                        );
                      } else if (!sameBytes(exported, keyData)) {
                        failures.push("private-matrix-roundtrip:" + algorithmLabel + ":" + format + ":" + usages.join("+"));
                      }
                    } else {
                      const error = await rejectionName(subtle.exportKey(format, key));
                      if (error !== "InvalidAccessError") {
                        failures.push("private-matrix-unextractable:" + algorithmLabel + ":" + format + ":" + usages.join("+") + ":" + error);
                      }
                    }
                  }
                }
              }
            }

            const importedRaw = await subtle.importKey("raw", publicRaw, "X25519", true, []);
            const importedSpki = await subtle.importKey("spki", spki, "X25519", true, []);
            const importedPkcs8 = await subtle.importKey("pkcs8", pkcs8, "X25519", true, ["deriveKey"]);
            const importedJwkPublic = await subtle.importKey("jwk", { ...jwkPublic, alg: "this is ignored" }, "X25519", true, []);
            // Chromium components/webcrypto/algorithms/x25519.cc delegates
            // JWK `use` validation to expected usages. Public X25519 imports
            // request no usages, so recognized but unrelated `use` values do
            // not make the public key inconsistent.
            const importedJwkPublicSigUse = await subtle.importKey("jwk", { ...jwkPublic, use: "sig" }, "X25519", true, []);
            const importedJwkPublicUnknownOps = await subtle.importKey(
              "jwk",
              { ...jwkPublic, key_ops: ["unknown", "also-unknown"] },
              "X25519",
              true,
              []
            );
            const importedJwkPrivate = await subtle.importKey("jwk", { ...jwkPrivate, alg: "this is ignored" }, "X25519", true, ["deriveBits"]);
            const importedPkcs8Duplicate = await subtle.importKey(
              "pkcs8",
              pkcs8,
              "X25519",
              true,
              ["deriveKey", "deriveBits", "deriveKey"]
            );
            const importedJwkPrivateDuplicate = await subtle.importKey(
              "jwk",
              jwkPrivate,
              "X25519",
              true,
              ["deriveBits", "deriveKey", "deriveBits"]
            );
            if (
              !sameBytes(await subtle.exportKey("raw", importedRaw), publicRaw) ||
              !sameBytes(await subtle.exportKey("spki", importedSpki), spki) ||
              !sameBytes(await subtle.exportKey("pkcs8", importedPkcs8), pkcs8) ||
              !sameBytes(await subtle.exportKey("raw", importedJwkPublicSigUse), publicRaw) ||
              !sameBytes(await subtle.exportKey("raw", importedJwkPublicUnknownOps), publicRaw) ||
              "alg" in (await subtle.exportKey("jwk", importedJwkPublic)) ||
              "alg" in (await subtle.exportKey("jwk", importedJwkPrivate))
            ) {
              failures.push("import-round-trip");
            }
            if (
              importedPkcs8Duplicate.usages.join(",") !== "deriveKey,deriveBits" ||
              importedJwkPrivateDuplicate.usages.join(",") !== "deriveKey,deriveBits"
            ) {
              failures.push("import-duplicate-usages");
            }

            // Chromium WPT: WebCryptoAPI/generateKey/failures_X25519.https.any.js.
            // This is the compact local equivalent of failures.js
            // invalidUsages(["deriveKey", "deriveBits"], ...): every
            // recognized non-X25519 usage must be a SyntaxError, including
            // otherwise-valid usage sets poisoned with one invalid usage.
            const invalidGenerateUsageCases = [
              ["encrypt"],
              ["decrypt"],
              ["sign"],
              ["verify"],
              ["wrapKey"],
              ["unwrapKey"],
              ["deriveBits", "encrypt"],
              ["deriveKey", "sign"],
              ["deriveKey", "deriveBits", "wrapKey"],
              ["deriveBits", "deriveKey", "deriveBits", "verify"]
            ];
            for (const algorithm of ["X25519", { name: "x25519" }]) {
              for (const usages of invalidGenerateUsageCases) {
                for (const extractable of [true, false]) {
                  const error = await rejectionName(subtle.generateKey(
                    algorithm,
                    extractable,
                    usages
                  ));
                  if (error !== "SyntaxError") {
                    failures.push("generate-invalid-usages:" + usages.join("+") + ":" + extractable + ":" + error);
                  }
                }
              }
            }
            const unrecognizedUsageError = await rejectionName(subtle.generateKey(
              {
                get name() {
                  failures.push("unexpected-name-getter-for-bad-usage");
                  return "X25519";
                }
              },
              true,
              ["encapsulateBits"]
            ));
            if (unrecognizedUsageError !== "TypeError") {
              failures.push("generate-unrecognized-usage:" + unrecognizedUsageError);
            }

            // These rejection cases are a compact local port of Chromium/WPT
            // WebCryptoAPI/import_export/importKey_failures.js scoped to
            // X25519. They pin the observable boundary between algorithm
            // normalization, key parsing, and usage/JWK metadata validation.
            const errors = await Promise.all([
              rejectionName(subtle.generateKey({ name: "X25519" }, true, [])),
              rejectionName(subtle.generateKey({ name: "X25519" }, true, ["sign"])),
              rejectionName(subtle.importKey("raw", publicRaw, {}, true, [])),
              rejectionName(subtle.importKey("spki", spki, {}, true, [])),
              rejectionName(subtle.importKey("pkcs8", pkcs8, {}, true, ["deriveBits"])),
              rejectionName(subtle.importKey("jwk", jwkPublic, {}, true, [])),
              rejectionName(subtle.importKey("raw", publicRaw, "X25519", true, ["deriveBits"])),
              rejectionName(subtle.importKey("spki", spki, "X25519", true, ["deriveKey"])),
              rejectionName(subtle.importKey("jwk", jwkPublic, "X25519", true, ["deriveBits"])),
              // Chromium components/webcrypto/algorithms/x25519.cc checks
              // raw/SPKI/PKCS8 key usages before parsing key bytes, and checks
              // JWK public/private usages after metadata but before x/d bytes.
              rejectionName(subtle.importKey("raw", publicRaw.slice(1), "X25519", true, ["deriveBits"])),
              rejectionName(subtle.importKey("spki", spki.slice(0, spki.length - 1), "X25519", true, ["deriveKey"])),
              rejectionName(subtle.importKey("pkcs8", pkcs8.slice(0, pkcs8.length - 1), "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { kty: "OKP", crv: "X25519" }, "X25519", true, ["deriveBits"])),
              rejectionName(subtle.importKey("raw", publicRaw.slice(1), "X25519", true, [])),
              rejectionName(subtle.importKey("spki", spki.slice(0, spki.length - 1), "X25519", true, [])),
              rejectionName(subtle.importKey("pkcs8", pkcs8.slice(0, pkcs8.length - 1), "X25519", true, ["deriveBits"])),
              rejectionName(subtle.importKey("jwk", { ...jwkPublic, x: "HPKx5gIuxTc3Htf1PlT6EVTYPpjrZOpR-uWzMHz-lw" }, "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { ...jwkPrivate, d: "yIOOdtBX37fYyVpp4TgWCt1jc_1xpNJ2u1bjqBtk_2" }, "X25519", true, ["deriveBits"])),
              rejectionName(subtle.importKey("jwk", { crv: "X25519", x: jwkPublic.x }, "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { kty: "OKP", x: jwkPublic.x }, "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { ...jwkPublic, kty: "EC" }, "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { ...jwkPublic, crv: "Ed25519" }, "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { ...jwkPublic, ext: false }, "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { ...jwkPublic, use: "invalid" }, "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { ...jwkPublic, key_ops: ["unknown", "unknown"] }, "X25519", true, [])),
              rejectionName(subtle.importKey("jwk", { ...jwkPrivate, ext: false }, "X25519", true, ["deriveBits"])),
              rejectionName(subtle.importKey("jwk", { ...jwkPrivate, key_ops: ["deriveBits"] }, "X25519", true, ["deriveKey"])),
              rejectionName(subtle.importKey("jwk", { ...jwkPrivate, key_ops: ["deriveBits", "deriveBits"] }, "X25519", true, ["deriveBits"])),
              rejectionName(subtle.importKey("jwk", { ...jwkPrivate, use: "sig" }, "X25519", true, ["deriveBits"])),
              rejectionName(subtle.importKey("jwk", { ...jwkPrivate, use: "invalid" }, "X25519", true, ["deriveBits"]))
            ]);
            const expectedErrors = [
              "SyntaxError",
              "SyntaxError",
              "TypeError",
              "TypeError",
              "TypeError",
              "TypeError",
              "SyntaxError",
              "SyntaxError",
              "SyntaxError",
              "SyntaxError",
              "SyntaxError",
              "SyntaxError",
              "SyntaxError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError",
              "DataError"
            ];
            if (errors.join(",") !== expectedErrors.join(",")) {
              failures.push("errors:" + errors.join(","));
            }
            globalThis.__cryptoX25519WptImportFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle X25519 WPT import probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoX25519WptImportFailures)")
        .expect("crypto subtle X25519 WPT import promise chain should settle");

    assert_eq!(result, "[]");
}
