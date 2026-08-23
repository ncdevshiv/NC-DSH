use super::*;

#[test]
fn crypto_subtle_hkdf_pbkdf2_match_chromium_wpt_vectors() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoKdfFailures = ["pending"];
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
            const asciiBytes = (value) => new Uint8Array(
              Array.from(value, (ch) => ch.charCodeAt(0))
            );
            const hexBytes = (value) => new Uint8Array(
              value.match(/../g).map((byte) => parseInt(byte, 16))
            );
            const checkDerivedBits = async (label, algorithm, key, lengthBits, expected) => {
              const actual = await subtle.deriveBits(algorithm, key, lengthBits);
              if (!sameBytes(actual, expected)) failures.push(label);
            };
            const emptyBytes = new Uint8Array([]);
            // Chromium WPT: WebCryptoAPI/derive_bits_keys/hkdf_vectors.js
            const hkdfBase = new Uint8Array([80, 64, 115, 115, 119, 48, 114, 100]);
            const hkdfSalt = new Uint8Array([
              83, 111, 100, 105, 117, 109, 32, 67, 104, 108, 111, 114,
              105, 100, 101, 32, 99, 111, 109, 112, 111, 117, 110, 100
            ]);
            const hkdfInfo = new Uint8Array([
              72, 75, 68, 70, 32, 101, 120, 116, 114, 97, 32, 105, 110, 102, 111
            ]);
            const hkdfExpected = new Uint8Array([
              42, 245, 144, 30, 40, 132, 156, 40, 68, 56, 87, 56, 106, 161,
              172, 59, 177, 39, 233, 38, 49, 193, 192, 81, 72, 45, 102,
              144, 148, 23, 114, 180
            ]);
            const hkdfSha1Expected = new Uint8Array([
              5, 173, 34, 237, 33, 56, 201, 96, 14, 77, 158, 39, 37,
              222, 211, 1, 245, 210, 135, 251, 251, 87, 2, 249, 153,
              188, 101, 54, 211, 237, 239, 152
            ]);
            const hkdfSha384EmptyInfoExpected = new Uint8Array([
              151, 96, 31, 78, 12, 83, 165, 211, 243, 162, 129, 0,
              153, 188, 104, 32, 236, 80, 8, 52, 52, 118, 155, 89,
              252, 36, 164, 23, 169, 84, 55, 52
            ]);
            const hkdfEmptySha512Expected = new Uint8Array([
              157, 115, 201, 142, 121, 30, 128, 235, 229, 180, 203, 69,
              105, 58, 163, 47, 221, 68, 181, 250, 62, 218, 179, 236,
              130, 249, 208, 244, 214, 105, 5, 226
            ]);
            // Chromium WPT: WebCryptoAPI/derive_bits_keys/derived_bits_length_vectors.js
            const hkdfLengthBase = new Uint8Array([
              85, 115, 101, 114, 115, 32, 115, 104, 111, 117, 108, 100, 32,
              112, 105, 99, 107, 32, 108, 111, 110, 103, 32, 112, 97, 115,
              115, 112, 104, 114, 97, 115, 101, 115, 32, 40, 110, 111, 116,
              32, 117, 115, 101, 32, 115, 104, 111, 114, 116, 32, 112, 97,
              115, 115, 119, 111, 114, 100, 115, 41, 33
            ]);
            const hkdfLengthExpected384 = new Uint8Array([
              49, 183, 214, 133, 48, 168, 99, 231, 23, 192, 129, 202, 105,
              23, 182, 134, 80, 179, 221, 154, 41, 243, 6, 6, 226, 202,
              209, 153, 190, 193, 77, 19, 165, 50, 181, 8, 254, 59, 122,
              199, 25, 224, 146, 248, 105, 105, 75, 84
            ]);
            // Chromium WPT: WebCryptoAPI/derive_bits_keys/pbkdf2_vectors.js
            const pbkdf2Salt = new Uint8Array([78, 97, 67, 108]);
            const pbkdf2Expected = new Uint8Array([
              198, 188, 85, 164, 4, 173, 206, 163, 106, 26, 181, 103, 152,
              8, 94, 10, 175, 105, 127, 107, 178, 193, 106, 80, 114, 248,
              56, 241, 125, 254, 108, 182
            ]);
            const pbkdf2Sha1Expected = new Uint8Array([
              70, 36, 219, 210, 19, 115, 238, 86, 89, 193, 37, 177,
              132, 238, 218, 162, 106, 51, 183, 124, 161, 19, 20, 185,
              240, 201, 218, 225, 228, 78, 155, 4
            ]);
            const pbkdf2Sha384ThousandExpected = new Uint8Array([
              170, 236, 90, 151, 109, 77, 53, 203, 32, 36, 72, 111,
              201, 249, 187, 154, 163, 234, 231, 206, 242, 188, 230, 38,
              100, 181, 179, 117, 28, 245, 15, 241
            ]);
            const pbkdf2Sha512LongThousandExpected = new Uint8Array([
              67, 225, 32, 36, 196, 211, 84, 114, 127, 126, 88, 132,
              44, 203, 96, 51, 161, 97, 214, 13, 197, 174, 81, 111,
              7, 110, 74, 88, 161, 136, 13, 56
            ]);

            const hkdfKey = await subtle.importKey("raw", hkdfBase, "HKDF", false, ["deriveBits", "deriveKey"]);
            const hkdfEmptyKey = await subtle.importKey("raw", emptyBytes, "HKDF", false, ["deriveBits"]);
            const hkdfLengthKey = await subtle.importKey("raw", hkdfLengthBase, "HKDF", false, ["deriveBits"]);
            const pbkdf2Key = await subtle.importKey("raw", hkdfBase, "PBKDF2", false, ["deriveBits", "deriveKey"]);
            const pbkdf2LongKey = await subtle.importKey("raw", hkdfLengthBase, "PBKDF2", false, ["deriveBits"]);
            const hkdfDuplicateKey = await subtle.importKey(
              "raw",
              hkdfBase,
              "HKDF",
              false,
              ["deriveBits", "deriveKey", "deriveBits"]
            );
            const pbkdf2DuplicateKey = await subtle.importKey(
              "raw",
              hkdfBase,
              "PBKDF2",
              false,
              ["deriveKey", "deriveBits", "deriveKey"]
            );
            if (
              hkdfDuplicateKey.usages.join(",") !== "deriveKey,deriveBits" ||
              pbkdf2DuplicateKey.usages.join(",") !== "deriveKey,deriveBits"
            ) {
              failures.push("kdf:duplicate-usages:import");
            }

            const hkdfBits = await subtle.deriveBits(
              { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
              hkdfKey,
              256
            );
            if (!sameBytes(hkdfBits, hkdfExpected)) failures.push("hkdf:deriveBits");
            const hkdfBits384 = await subtle.deriveBits(
              { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
              hkdfLengthKey,
              384
            );
            if (!sameBytes(hkdfBits384, hkdfLengthExpected384)) failures.push("hkdf:deriveBits:384");
            const hkdfEmpty = await subtle.deriveBits(
              { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
              hkdfKey,
              0
            );
            if (new Uint8Array(hkdfEmpty).byteLength !== 0) failures.push("hkdf:zero-length");
            // Chromium legacy test: crypto/subtle/hkdf/deriveBits.html.
            // This keeps the Blink empty salt/info boundary covered alongside
            // the larger WPT and RFC5869 HKDF vectors below.
            const legacyHkdfKey = await subtle.importKey(
              "raw",
              hexBytes("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"),
              "HKDF",
              false,
              ["deriveKey", "deriveBits"]
            );
            const legacyHkdfAlgorithm = {
              name: "HKDF",
              hash: "SHA-256",
              salt: emptyBytes,
              info: emptyBytes
            };
            const legacyHkdfEmpty = await subtle.deriveBits(
              legacyHkdfAlgorithm,
              legacyHkdfKey,
              0
            );
            if (new Uint8Array(legacyHkdfEmpty).byteLength !== 0) {
              failures.push("hkdf:legacy-zero-length");
            }
            await checkDerivedBits(
              "hkdf:legacy-one-byte",
              legacyHkdfAlgorithm,
              legacyHkdfKey,
              8,
              new Uint8Array([141])
            );

            const pbkdf2Bits = await subtle.deriveBits(
              { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" },
              pbkdf2Key,
              256
            );
            if (!sameBytes(pbkdf2Bits, pbkdf2Expected)) failures.push("pbkdf2:deriveBits");
            // Chromium WPT: WebCryptoAPI/derive_bits_keys/derived_bits_length.
            // KDF algorithms require an explicit nullable length, reject
            // null/undefined/omitted lengths, and accept zero-length output.
            // The 100000-iteration PBKDF2 384-bit vector lives in
            // moli-webcrypto's Rust tests so the browser VM checkpoint
            // can keep catching runaway microtasks.
            const pbkdf2Empty = await subtle.deriveBits(
              { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" },
              pbkdf2Key,
              0
            );
            if (new Uint8Array(pbkdf2Empty).byteLength !== 0) failures.push("pbkdf2:zero-length");
            await checkDerivedBits(
              "hkdf:deriveBits:sha1",
              { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-1" },
              hkdfKey,
              256,
              hkdfSha1Expected
            );
            await checkDerivedBits(
              "hkdf:deriveBits:sha384-empty-info",
              { name: "HKDF", salt: hkdfSalt, info: emptyBytes, hash: "SHA-384" },
              hkdfKey,
              256,
              hkdfSha384EmptyInfoExpected
            );
            await checkDerivedBits(
              "hkdf:deriveBits:empty-sha512",
              { name: "HKDF", salt: emptyBytes, info: emptyBytes, hash: "SHA-512" },
              hkdfEmptyKey,
              256,
              hkdfEmptySha512Expected
            );
            await checkDerivedBits(
              "pbkdf2:deriveBits:sha1",
              { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-1" },
              pbkdf2Key,
              256,
              pbkdf2Sha1Expected
            );
            await checkDerivedBits(
              "pbkdf2:deriveBits:sha384-1000",
              { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1000, hash: "SHA-384" },
              pbkdf2Key,
              256,
              pbkdf2Sha384ThousandExpected
            );
            await checkDerivedBits(
              "pbkdf2:deriveBits:sha512-long-1000",
              { name: "PBKDF2", salt: hkdfSalt, iterations: 1000, hash: "SHA-512" },
              pbkdf2LongKey,
              256,
              pbkdf2Sha512LongThousandExpected
            );
            // Chromium legacy test: crypto/subtle/pbkdf2/deriveBits.html.
            // These cases exercise WebCrypto's raw password import path with
            // non-text bytes, embedded NUL bytes, empty salts, and empty
            // passwords rather than only the common ASCII password fixtures.
            const legacyPbkdf2Cases = [
              {
                label: "non-ascii-password",
                password: new Uint8Array([200, 201, 202, 203, 204, 205, 206, 207]),
                salt: asciiBytes("salt"),
                iterations: 20,
                hash: "SHA-1",
                expected: "a7950c143ec64e2b8d4bb1db8677188b"
              },
              {
                label: "empty-salt",
                password: asciiBytes("pass\0word"),
                salt: emptyBytes,
                iterations: 20,
                hash: "SHA-1",
                expected: "7deaf8b4a801011c1cd27f36e3bfc962"
              },
              {
                label: "sha256",
                password: asciiBytes("password"),
                salt: asciiBytes("salt"),
                iterations: 20,
                hash: "SHA-256",
                expected: "83eb100b6a3a975f0fe3ffcdc2419852"
              },
              {
                label: "sha512",
                password: asciiBytes("password"),
                salt: asciiBytes("salt"),
                iterations: 20,
                hash: "SHA-512",
                expected: "e4dfce3830983830c50c351a0b0f79e1"
              },
              {
                label: "empty-password-sha384",
                password: emptyBytes,
                salt: asciiBytes("salt"),
                iterations: 20,
                hash: "SHA-384",
                expected: "750261780a187897a9978371599db5d1"
              }
            ];
            for (const vector of legacyPbkdf2Cases) {
              const key = await subtle.importKey(
                "raw",
                vector.password,
                "PBKDF2",
                false,
                ["deriveBits", "deriveKey"]
              );
              if (
                key.type !== "secret" ||
                key.extractable !== false ||
                key.algorithm.name !== "PBKDF2" ||
                key.usages.join(",") !== "deriveKey,deriveBits"
              ) {
                failures.push(`pbkdf2:legacy-key:${vector.label}`);
              }
              await checkDerivedBits(
                `pbkdf2:legacy:${vector.label}`,
                {
                  name: "PBKDF2",
                  salt: vector.salt,
                  iterations: vector.iterations,
                  hash: vector.hash
                },
                key,
                128,
                hexBytes(vector.expected)
              );
            }
            // Chromium legacy test: crypto/subtle/pbkdf2/deriveKey-aes.html.
            // The original fixture derives AES-CBC-128 keys from RFC6070
            // PBKDF2 vectors and then exports the derived key bytes. This
            // exercises Moli's key-management path without requiring AES
            // encryption/decryption primitives.
            const legacyPbkdf2AesCases = [
              {
                label: "rfc6070-1",
                password: asciiBytes("password"),
                salt: asciiBytes("salt"),
                iterations: 1,
                hash: "SHA-1",
                expected: "0c60c80f961f0e71f3a9b524af6012062fe037a6"
              },
              {
                label: "rfc6070-2",
                password: asciiBytes("password"),
                salt: asciiBytes("salt"),
                iterations: 2,
                hash: "SHA-1",
                expected: "ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957"
              },
              {
                label: "rfc6070-4096",
                password: asciiBytes("password"),
                salt: asciiBytes("salt"),
                iterations: 4096,
                hash: "SHA-1",
                expected: "4b007901b765489abead49d926f721d065a429c1"
              },
              {
                label: "rfc6070-long",
                password: asciiBytes("passwordPASSWORDpassword"),
                salt: asciiBytes("saltSALTsaltSALTsaltSALTsaltSALTsalt"),
                iterations: 4096,
                hash: "SHA-1",
                expected: "3d2eec4fe41c849b80c8d83662c0e44a8b291a964cf2f07038"
              },
              {
                label: "rfc6070-nul",
                password: asciiBytes("pass\0word"),
                salt: asciiBytes("sa\0lt"),
                iterations: 4096,
                hash: "SHA-1",
                expected: "56fa6aa75548099dcc37d7f03425e0c3"
              }
            ];
            for (const vector of legacyPbkdf2AesCases) {
              const key = await subtle.importKey(
                "raw",
                vector.password,
                "PBKDF2",
                false,
                ["deriveBits", "deriveKey"]
              );
              if (
                key.type !== "secret" ||
                key.extractable !== false ||
                key.algorithm.name !== "PBKDF2" ||
                key.usages.join(",") !== "deriveKey,deriveBits"
              ) {
                failures.push(`pbkdf2:legacy-aes-key:${vector.label}`);
              }
              const derivedAes = await subtle.deriveKey(
                {
                  name: "PBKDF2",
                  salt: vector.salt,
                  iterations: vector.iterations,
                  hash: { name: vector.hash }
                },
                key,
                { name: "aes-cbc", length: 128 },
                true,
                ["encrypt"]
              );
              if (
                derivedAes.type !== "secret" ||
                derivedAes.extractable !== true ||
                derivedAes.algorithm.name !== "AES-CBC" ||
                derivedAes.algorithm.length !== 128 ||
                derivedAes.usages.join(",") !== "encrypt" ||
                !sameBytes(await subtle.exportKey("raw", derivedAes), hexBytes(vector.expected.slice(0, 32)))
              ) {
                failures.push(`pbkdf2:legacy-aes:${vector.label}`);
              }
            }

            const hkdfHmac = await subtle.deriveKey(
              { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
              hkdfKey,
              { name: "HMAC", hash: "SHA-256", length: 128 },
              true,
              ["sign", "verify"]
            );
            if (
              hkdfHmac.algorithm.name !== "HMAC" ||
              hkdfHmac.algorithm.hash.name !== "SHA-256" ||
              hkdfHmac.algorithm.length !== 128 ||
              !sameBytes(await subtle.exportKey("raw", hkdfHmac), hkdfExpected.slice(0, 16))
            ) {
              failures.push("hkdf:deriveKey:hmac");
            }
            const duplicateHkdfHmac = await subtle.deriveKey(
              { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
              hkdfDuplicateKey,
              { name: "HMAC", hash: "SHA-256", length: 128 },
              true,
              ["verify", "verify", "sign"]
            );
            if (
              duplicateHkdfHmac.usages.join(",") !== "sign,verify" ||
              !sameBytes(await subtle.exportKey("raw", duplicateHkdfHmac), hkdfExpected.slice(0, 16))
            ) {
              failures.push("hkdf:deriveKey:duplicate-usages");
            }
            const hkdfAesKw = await subtle.deriveKey(
              { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
              hkdfKey,
              { name: "AES-KW", length: 256 },
              true,
              ["wrapKey", "unwrapKey"]
            );
            if (
              hkdfAesKw.algorithm.name !== "AES-KW" ||
              hkdfAesKw.algorithm.length !== 256 ||
              hkdfAesKw.usages.join(",") !== "wrapKey,unwrapKey" ||
              !sameBytes(await subtle.exportKey("raw", hkdfAesKw), hkdfExpected)
            ) {
              failures.push("hkdf:deriveKey:aes-kw");
            }

            const pbkdf2Aes = await subtle.deriveKey(
              { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" },
              pbkdf2Key,
              { name: "AES-GCM", length: 128 },
              true,
              ["encrypt", "decrypt"]
            );
            if (
              pbkdf2Aes.algorithm.name !== "AES-GCM" ||
              pbkdf2Aes.algorithm.length !== 128 ||
              !sameBytes(await subtle.exportKey("raw", pbkdf2Aes), pbkdf2Expected.slice(0, 16))
            ) {
              failures.push("pbkdf2:deriveKey:aes");
            }
            const duplicatePbkdf2Aes = await subtle.deriveKey(
              { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" },
              pbkdf2DuplicateKey,
              { name: "AES-GCM", length: 128 },
              true,
              ["decrypt", "encrypt", "decrypt"]
            );
            if (
              duplicatePbkdf2Aes.usages.join(",") !== "encrypt,decrypt" ||
              !sameBytes(await subtle.exportKey("raw", duplicatePbkdf2Aes), pbkdf2Expected.slice(0, 16))
            ) {
              failures.push("pbkdf2:deriveKey:duplicate-usages");
            }
            const pbkdf2HmacSha512 = await subtle.deriveKey(
              { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" },
              pbkdf2Key,
              { name: "HMAC", hash: "SHA-512", length: 256 },
              true,
              ["verify"]
            );
            if (
              pbkdf2HmacSha512.algorithm.name !== "HMAC" ||
              pbkdf2HmacSha512.algorithm.hash.name !== "SHA-512" ||
              pbkdf2HmacSha512.algorithm.length !== 256 ||
              pbkdf2HmacSha512.usages.join(",") !== "verify" ||
              !sameBytes(await subtle.exportKey("raw", pbkdf2HmacSha512), pbkdf2Expected)
            ) {
              failures.push("pbkdf2:deriveKey:hmac-sha512");
            }

            const noBitsKey = await subtle.importKey("raw", hkdfBase, "HKDF", false, ["deriveKey"]);
            const noKeyKey = await subtle.importKey("raw", hkdfBase, "PBKDF2", false, ["deriveBits"]);
            const wrongKey = await subtle.importKey("raw", hkdfBase, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
            const errors = await Promise.all([
              rejectionName(subtle.importKey("raw", hkdfBase, "HKDF", true, ["deriveBits"])),
              rejectionName(subtle.importKey("raw", hkdfBase, "PBKDF2", false, [])),
              // Chromium legacy tests:
              // crypto/subtle/hkdf/importKey-failures.html and
              // crypto/subtle/pbkdf2/importKey-failures.html.
              rejectionName(subtle.importKey("jwk", { kty: "HKDF" }, "HKDF", false, ["deriveKey"])),
              rejectionName(subtle.importKey("spki", hkdfBase, "HKDF", false, ["deriveKey"])),
              rejectionName(subtle.importKey("jwk", { kty: "PBKDF2" }, "PBKDF2", false, ["deriveKey"])),
              rejectionName(subtle.importKey("spki", hkdfBase, "PBKDF2", false, ["deriveKey"])),
              rejectionName(subtle.exportKey("raw", hkdfKey)),
              // Chromium legacy test: crypto/subtle/hkdf/deriveKey.html.
              // Deriving another KDF key from a KDF source has no target key
              // length, so deriveBits fails with OperationError.
              rejectionName(subtle.deriveKey(
                { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
                hkdfKey,
                "HKDF",
                false,
                ["deriveKey"]
              )),
              rejectionName(subtle.deriveKey(
                { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" },
                pbkdf2Key,
                "PBKDF2",
                false,
                ["deriveBits"]
              )),
              // Chromium legacy test:
              // crypto/subtle/pbkdf2/deriveKey-failures.html.
              rejectionName(subtle.deriveKey(
                { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
                hkdfKey,
                { name: "AES-GCM" },
                true,
                ["encrypt"]
              )),
              rejectionName(subtle.deriveKey(
                { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
                hkdfKey,
                { name: "AES-GCM", length: 120 },
                true,
                ["encrypt"]
              )),
              rejectionName(subtle.deriveKey(
                { name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" },
                hkdfKey,
                { name: "AES-GCM", length: 70000 },
                true,
                ["encrypt"]
              )),
              rejectionName(subtle.deriveBits({ name: "HKDF", info: hkdfInfo, hash: "SHA-256" }, hkdfKey, 256)),
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: hkdfSalt, hash: "SHA-256" }, hkdfKey, 256)),
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA256" }, hkdfKey, 256)),
              // Chromium legacy test:
              // crypto/subtle/hkdf/deriveBits-failures.html.
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" }, wrongKey, 8)),
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: new Uint8Array(), info: new Uint8Array(), hash: "SHA-256" }, hkdfKey, 65288)),
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: new Uint8Array(), info: new Uint8Array(), hash: "SHA-256" }, hkdfKey, 15)),
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" }, noBitsKey, 256)),
              rejectionName(subtle.deriveKey(
                { name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" },
                noKeyKey,
                { name: "HMAC", hash: "SHA-256", length: 128 },
                true,
                ["sign"]
              )),
              // Chromium legacy test:
              // crypto/subtle/pbkdf2/deriveBits-failures.html.
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 0, hash: "SHA-256" }, pbkdf2Key, 256)),
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: -10, hash: "SHA-256" }, pbkdf2Key, 16)),
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" }, pbkdf2Key, 44)),
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" }, pbkdf2Key, -10)),
              // Moli product limits: keep hostile pages from using
              // synchronous WebCrypto to force long PBKDF2 loops or large
              // renderer allocations before WebCryptoTaskOwner lands.
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 1000001, hash: "SHA-256" }, pbkdf2Key, 128)),
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" }, pbkdf2Key, 8388616)),
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" }, hkdfKey, null)),
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" }, hkdfKey, undefined)),
              rejectionName(subtle.deriveBits({ name: "HKDF", salt: hkdfSalt, info: hkdfInfo, hash: "SHA-256" }, hkdfKey)),
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" }, pbkdf2Key, null)),
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" }, pbkdf2Key, undefined)),
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" }, pbkdf2Key)),
              rejectionName(subtle.deriveBits({ name: "PBKDF2", salt: pbkdf2Salt, iterations: 1, hash: "SHA-256" }, wrongKey, 256))
            ]);
            const expectedErrors = [
              "SyntaxError",
              "SyntaxError",
              "NotSupportedError",
              "NotSupportedError",
              "NotSupportedError",
              "NotSupportedError",
              "InvalidAccessError",
              "OperationError",
              "OperationError",
              "TypeError",
              "OperationError",
              "TypeError",
              "TypeError",
              "TypeError",
              "NotSupportedError",
              "InvalidAccessError",
              "OperationError",
              "OperationError",
              "InvalidAccessError",
              "InvalidAccessError",
              "OperationError",
              "TypeError",
              "OperationError",
              "OperationError",
              "OperationError",
              "OperationError",
              "OperationError",
              "OperationError",
              "OperationError",
              "OperationError",
              "OperationError",
              "OperationError",
              "InvalidAccessError"
            ];
            if (errors.join(",") !== expectedErrors.join(",")) {
              failures.push("errors:" + errors.join(","));
            }
            globalThis.__cryptoKdfFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle KDF probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoKdfFailures)")
        .expect("crypto subtle KDF promise chain should settle");

    assert_eq!(result, "[]");
}
#[test]
fn crypto_subtle_pbkdf2_params_follow_chromium_member_order() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoPbkdf2ParamOrderProbe = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const baseKey = await subtle.importKey(
              "raw",
              new Uint8Array([1, 2, 3, 4]),
              "PBKDF2",
              false,
              ["deriveBits", "deriveKey"]
            );
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const orderedAlgorithm = (events) => ({
              get name() {
                events.push("name");
                return "PBKDF2";
              },
              get salt() {
                events.push("salt");
                return new Uint8Array([5, 6, 7, 8]);
              },
              get iterations() {
                events.push("iterations");
                return 1;
              },
              get hash() {
                events.push("hash");
                return "SHA-256";
              }
            });

            // Chromium NormalizeAlgorithm parses Pbkdf2Params as salt,
            // iterations, then hash after the inherited name member.
            const deriveBitsEvents = [];
            await subtle.deriveBits(orderedAlgorithm(deriveBitsEvents), baseKey, 128);

            const deriveKeyEvents = [];
            await subtle.deriveKey(
              orderedAlgorithm(deriveKeyEvents),
              baseKey,
              { name: "HMAC", hash: "SHA-256", length: 128 },
              true,
              ["sign"]
            );

            const missingSaltEvents = [];
            const missingSalt = await rejectionName(subtle.deriveBits(
              {
                get name() {
                  missingSaltEvents.push("name");
                  return "PBKDF2";
                },
                get iterations() {
                  missingSaltEvents.push("iterations");
                  return 1;
                },
                get hash() {
                  missingSaltEvents.push("hash");
                  return "MD5";
                }
              },
              baseKey,
              128
            ));

            const zeroIterationEvents = [];
            const zeroIterations = await rejectionName(subtle.deriveBits(
              {
                get name() {
                  zeroIterationEvents.push("name");
                  return "PBKDF2";
                },
                get salt() {
                  zeroIterationEvents.push("salt");
                  return new Uint8Array([5, 6, 7, 8]);
                },
                get iterations() {
                  zeroIterationEvents.push("iterations");
                  return 0;
                },
                get hash() {
                  zeroIterationEvents.push("hash");
                  return "MD5";
                }
              },
              baseKey,
              128
            ));

            globalThis.__cryptoPbkdf2ParamOrderProbe = [
              deriveBitsEvents.join(","),
              deriveKeyEvents.join(","),
              missingSalt + ":" + missingSaltEvents.join(","),
              zeroIterations + ":" + zeroIterationEvents.join(",")
            ];
          })();
        })()
        "#,
    )
    .expect("crypto subtle PBKDF2 param-order probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoPbkdf2ParamOrderProbe)")
        .expect("crypto subtle PBKDF2 param-order promise chain should settle");

    assert_eq!(
        result,
        r#"["name,salt,iterations,hash","name,salt,iterations,hash","TypeError:name","OperationError:name,salt,iterations"]"#
    );
}
