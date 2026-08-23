use super::*;

#[test]
fn crypto_subtle_digest_failures_match_chromium_legacy() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoDigestFailureProbe = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const data = new Uint8Array([0]);
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );

            // Chromium legacy test:
            // crypto/subtle/digest-failures.html.
            globalThis.__cryptoDigestFailureProbe = await Promise.all([
              rejectionName(subtle.digest({ name: "sha-1" })),
              rejectionName(subtle.digest({ name: "sha-1" }, null)),
              rejectionName(subtle.digest({ name: "sha-1" }, 10)),
              rejectionName(subtle.digest(null, data)),
              rejectionName(subtle.digest({ name: "sha" }, data)),
              rejectionName(subtle.digest({}, data))
            ]);
          })();
        })()
        "#,
    )
    .expect("crypto subtle digest failure probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoDigestFailureProbe)")
        .expect("crypto subtle digest failure promise chain should settle");

    assert_eq!(
        result,
        r#"["TypeError","TypeError","TypeError","NotSupportedError","NotSupportedError","TypeError"]"#
    );
}
#[test]
fn crypto_subtle_digest_matches_chromium_wpt_vectors() {
    let mut vm = new_storage_test_vm("https://example.com/");

    vm.eval(
        r#"
        (() => {
          globalThis.__cryptoDigestWptFailures = ["pending"];
          (async () => {
            const subtle = crypto.subtle;
            const failures = [];
            const sameBytes = (left, right) => {
              const a = new Uint8Array(left);
              const b = new Uint8Array(right);
              return a.length === b.length && a.every((value, index) => value === b[index]);
            };
            const copyBytes = (bytes) => {
              const copy = new Uint8Array(bytes.byteLength);
              copy.set(bytes);
              return copy;
            };
            const rejectionName = (promise) => promise.then(
              () => "resolved",
              (error) => error.name
            );
            const hexBytes = (value) => value.length === 0
              ? new Uint8Array([])
              : new Uint8Array(value.match(/../g).map((byte) => parseInt(byte, 16)));

            // Chromium WPT: WebCryptoAPI/digest/digest.https.any.js
            const sourceData = {
              empty: new Uint8Array([]),
              short: new Uint8Array([
                21, 110, 234, 124, 193, 76, 86, 203, 148, 219, 3, 10, 74, 157, 149, 255
              ]),
              medium: new Uint8Array([
                182, 200, 249, 223, 100, 140, 208, 136, 183, 15, 56, 231, 65, 151, 177,
                140, 184, 30, 30, 67, 80, 213, 11, 204, 184, 251, 90, 115, 121, 200,
                123, 178, 227, 214, 237, 84, 97, 237, 30, 159, 54, 243, 64, 163, 150,
                42, 68, 107, 129, 91, 121, 75, 75, 212, 58, 68, 3, 80, 32, 119, 178,
                37, 108, 200, 7, 131, 127, 58, 172, 209, 24, 235, 75, 156, 43, 174,
                184, 151, 6, 134, 37, 171, 172, 161, 147
              ])
            };
            sourceData.long = new Uint8Array(1024 * sourceData.medium.byteLength);
            for (let index = 0; index < 1024; index += 1) {
              sourceData.long.set(sourceData.medium, index * sourceData.medium.byteLength);
            }

            const expected = {
              "sha-1": {
                empty: [218, 57, 163, 238, 94, 107, 75, 13, 50, 85, 191, 239, 149, 96, 24, 144, 175, 216, 7, 9],
                short: [201, 19, 24, 205, 242, 57, 106, 1, 94, 63, 78, 106, 134, 160, 186, 101, 184, 99, 89, 68],
                medium: [229, 65, 6, 8, 112, 235, 22, 191, 51, 182, 142, 81, 245, 19, 82, 104, 147, 152, 103, 41],
                long: [48, 152, 181, 0, 55, 236, 208, 46, 189, 101, 118, 83, 178, 191, 160, 30, 238, 39, 162, 234]
              },
              "sha-256": {
                empty: [227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39, 174, 65, 228, 100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85],
                short: [162, 131, 17, 134, 152, 71, 146, 199, 211, 45, 89, 200, 151, 64, 104, 127, 25, 173, 220, 27, 149, 158, 113, 161, 204, 83, 138, 59, 126, 216, 67, 242],
                medium: [83, 83, 103, 135, 126, 240, 20, 215, 252, 113, 126, 92, 183, 132, 62, 89, 182, 26, 238, 98, 199, 2, 156, 236, 126, 198, 193, 47, 217, 36, 224, 228],
                long: [20, 205, 234, 157, 199, 95, 90, 98, 116, 217, 252, 30, 100, 0, 153, 18, 241, 220, 211, 6, 180, 143, 232, 233, 207, 18, 45, 230, 113, 87, 23, 129]
              },
              "sha-384": {
                empty: [56, 176, 96, 167, 81, 172, 150, 56, 76, 217, 50, 126, 177, 177, 227, 106, 33, 253, 183, 17, 20, 190, 7, 67, 76, 12, 199, 191, 99, 246, 225, 218, 39, 78, 222, 191, 231, 111, 101, 251, 213, 26, 210, 241, 72, 152, 185, 91],
                short: [107, 245, 234, 101, 36, 209, 205, 220, 67, 247, 207, 59, 86, 238, 5, 146, 39, 64, 74, 47, 83, 143, 2, 42, 61, 183, 68, 122, 120, 44, 6, 193, 237, 5, 232, 171, 79, 94, 220, 23, 243, 113, 20, 64, 223, 233, 119, 49],
                medium: [203, 194, 197, 136, 254, 91, 37, 249, 22, 218, 40, 180, 228, 122, 72, 74, 230, 252, 31, 228, 144, 45, 213, 201, 147, 154, 107, 253, 3, 74, 179, 180, 139, 57, 8, 116, 54, 1, 31, 106, 153, 135, 157, 39, 149, 64, 233, 119],
                long: [73, 244, 253, 179, 152, 25, 104, 249, 125, 87, 55, 15, 133, 52, 80, 103, 205, 82, 150, 169, 125, 209, 161, 142, 6, 145, 30, 117, 110, 150, 8, 73, 37, 41, 135, 14, 26, 209, 48, 153, 141, 87, 203, 251, 183, 193, 208, 158]
              },
              "sha-512": {
                empty: [207, 131, 225, 53, 126, 239, 184, 189, 241, 84, 40, 80, 214, 109, 128, 7, 214, 32, 228, 5, 11, 87, 21, 220, 131, 244, 169, 33, 211, 108, 233, 206, 71, 208, 209, 60, 93, 133, 242, 176, 255, 131, 24, 210, 135, 126, 236, 47, 99, 185, 49, 189, 71, 65, 122, 129, 165, 56, 50, 122, 249, 39, 218, 62],
                short: [55, 82, 72, 190, 95, 243, 75, 231, 76, 171, 79, 241, 195, 188, 141, 198, 139, 213, 248, 223, 244, 2, 62, 152, 248, 123, 134, 92, 255, 44, 114, 66, 146, 223, 24, 148, 67, 166, 79, 244, 19, 74, 101, 205, 70, 53, 185, 212, 245, 220, 13, 63, 182, 117, 40, 0, 42, 99, 172, 242, 108, 157, 165, 117],
                medium: [185, 16, 159, 131, 158, 142, 164, 60, 137, 15, 41, 60, 225, 29, 198, 226, 121, 141, 30, 36, 49, 241, 228, 185, 25, 227, 178, 12, 79, 54, 48, 59, 163, 156, 145, 109, 179, 6, 196, 90, 59, 101, 118, 31, 245, 190, 133, 50, 142, 234, 244, 44, 56, 48, 241, 217, 94, 122, 65, 22, 91, 125, 45, 54],
                long: [75, 2, 202, 246, 80, 39, 96, 48, 234, 86, 23, 229, 151, 197, 213, 63, 217, 218, 166, 139, 120, 191, 230, 11, 34, 170, 184, 211, 106, 76, 42, 58, 255, 219, 113, 35, 79, 73, 39, 103, 55, 197, 117, 221, 247, 77, 20, 5, 76, 189, 111, 219, 152, 253, 13, 220, 188, 180, 111, 145, 173, 118, 182, 238]
              }
            };

            for (const [size, source] of Object.entries(sourceData)) {
              for (const [algorithm, expectedBySize] of Object.entries(expected)) {
                const upper = algorithm.toUpperCase();
                const lower = algorithm.toLowerCase();
                const mixed = upper.slice(0, 1) + lower.slice(1);
                for (const name of [upper, lower, mixed]) {
                  const digest = await subtle.digest({ name }, source);
                  if (!sameBytes(digest, new Uint8Array(expectedBySize[size]))) {
                    failures.push(`${algorithm}:${size}:${name}`);
                  }
                }
                if (source.byteLength > 0) {
                  const afterCallCopy = copyBytes(source);
                  const promise = subtle.digest({ name: upper }, afterCallCopy);
                  afterCallCopy[0] = 255 - afterCallCopy[0];
                  if (!sameBytes(await promise, new Uint8Array(expectedBySize[size]))) {
                    failures.push(`${algorithm}:${size}:after-call-copy`);
                  }

                  // WPT WebCryptoAPI/digest/digest.https.any.js: the
                  // algorithm dictionary is normalized before BufferSource
                  // operation data is converted, so getter side effects during
                  // the call are visible to the data snapshot.
                  const duringCallCopy = copyBytes(source);
                  duringCallCopy[0] = 255 - duringCallCopy[0];
                  const digest = await subtle.digest({
                    get name() {
                      duringCallCopy[0] = source[0];
                      return upper;
                    }
                  }, duringCallCopy);
                  if (!sameBytes(digest, new Uint8Array(expectedBySize[size]))) {
                    failures.push(`${algorithm}:${size}:during-call-copy`);
                  }
                }
              }
            }
            // Chromium legacy test: crypto/subtle/sha/digest.html. These NIST
            // byte-test vectors keep exact empty, single-NUL, and short
            // incremental byte inputs covered for every supported SHA family.
            const legacyShaVectors = [
              {
                algorithm: "SHA-1",
                input: "",
                output: "da39a3ee5e6b4b0d3255bfef95601890afd80709"
              },
              {
                algorithm: "SHA-256",
                input: "",
                output: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
              },
              {
                algorithm: "SHA-384",
                input: "",
                output: "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b"
              },
              {
                algorithm: "SHA-512",
                input: "",
                output: "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
              },
              {
                algorithm: "SHA-1",
                input: "00",
                output: "5ba93c9db0cff93f52b521d7420e43f6eda2784f"
              },
              {
                algorithm: "SHA-256",
                input: "00",
                output: "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d"
              },
              {
                algorithm: "SHA-384",
                input: "00",
                output: "bec021b4f368e3069134e012c2b4307083d3a9bdd206e24e5f0d86e13d6636655933ec2b413465966817a9c208a11717"
              },
              {
                algorithm: "SHA-512",
                input: "00",
                output: "b8244d028981d693af7b456af8efa4cad63d282e19ff14942c246e50d9351d22704a802a71c3580b6370de4ceb293c324a8423342557d4e5c38438f0e36910ee"
              },
              {
                algorithm: "SHA-1",
                input: "000102030405",
                output: "868460d98d09d8bbb93d7b6cdd15cc7fbec676b9"
              },
              {
                algorithm: "SHA-256",
                input: "000102030405",
                output: "17e88db187afd62c16e5debf3e6527cd006bc012bc90b51a810cd80c2d511f43"
              },
              {
                algorithm: "SHA-384",
                input: "000102030405",
                output: "79f4738706fce9650ac60266675c3cd07298b09923850d525604d040e6e448adc7dc22780d7e1b95bfeaa86a678e4552"
              },
              {
                algorithm: "SHA-512",
                input: "000102030405",
                output: "2f3831bccc94cf061bcfa5f8c23c1429d26e3bc6b76edad93d9025cb91c903af6cf9c935dc37193c04c2c66e7d9de17c358284418218afea2160147aaa912f4c"
              }
            ];
            for (const vector of legacyShaVectors) {
              const digest = await subtle.digest(
                { name: vector.algorithm },
                hexBytes(vector.input)
              );
              if (!sameBytes(digest, hexBytes(vector.output))) {
                failures.push(`legacy:${vector.algorithm}:${vector.input}`);
              }
            }

            const errors = await Promise.all([
              rejectionName(subtle.digest({ name: "AES-GCM" }, sourceData.short)),
              rejectionName(subtle.digest({ name: "PBKDF2" }, sourceData.short)),
              rejectionName(subtle.digest({}, sourceData.short)),
              rejectionName(subtle.digest({ name: undefined }, sourceData.short))
            ]);
            if (errors.join(",") !== "NotSupportedError,NotSupportedError,TypeError,TypeError") {
              failures.push("errors:" + errors.join(","));
            }
            globalThis.__cryptoDigestWptFailures = failures;
          })();
        })()
        "#,
    )
    .expect("crypto subtle digest WPT probe should evaluate");

    let result = vm
        .eval("JSON.stringify(globalThis.__cryptoDigestWptFailures)")
        .expect("crypto subtle digest WPT promise chain should settle");

    assert_eq!(result, "[]");
}
