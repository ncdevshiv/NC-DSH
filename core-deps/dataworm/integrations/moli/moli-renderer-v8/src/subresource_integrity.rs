use base64::Engine;
use moli_crypto::DigestAlgorithm;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SubresourceIntegrityAlgorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl SubresourceIntegrityAlgorithm {
    fn output_len_bytes(self) -> usize {
        self.digest_algorithm().output_len_bytes()
    }

    fn digest_algorithm(self) -> DigestAlgorithm {
        match self {
            Self::Sha256 => DigestAlgorithm::Sha256,
            Self::Sha384 => DigestAlgorithm::Sha384,
            Self::Sha512 => DigestAlgorithm::Sha512,
        }
    }
}

struct ParsedIntegrityMetadata {
    tokens: Vec<ParsedIntegrityToken>,
    saw_supported_algorithm: bool,
}

struct ParsedIntegrityToken {
    algorithm: SubresourceIntegrityAlgorithm,
    expected_digest: Vec<u8>,
}

pub(crate) fn observe_subresource_integrity_metadata(integrity: Option<&str>) {
    let Some(integrity) = integrity
        .map(str::trim)
        .filter(|integrity| !integrity.is_empty())
    else {
        return;
    };
    let metadata = parse_integrity_metadata(integrity);
    let _strongest = metadata
        .tokens
        .iter()
        .map(|token| token.algorithm)
        .max_by_key(|algorithm| algorithm.output_len_bytes());
    let _saw_supported_algorithm = metadata.saw_supported_algorithm;
    let _has_decoded_digest = metadata
        .tokens
        .iter()
        .any(|token| !token.expected_digest.is_empty());
}

fn parse_integrity_metadata(integrity: &str) -> ParsedIntegrityMetadata {
    let mut saw_supported_algorithm = false;
    let tokens = integrity
        .split_ascii_whitespace()
        .filter_map(|token| {
            let parsed = parse_integrity_token(token);
            saw_supported_algorithm |= parsed.saw_supported_algorithm;
            parsed.token
        })
        .collect();
    ParsedIntegrityMetadata {
        tokens,
        saw_supported_algorithm,
    }
}

struct ParsedIntegrityTokenResult {
    token: Option<ParsedIntegrityToken>,
    saw_supported_algorithm: bool,
}

fn parse_integrity_token(token: &str) -> ParsedIntegrityTokenResult {
    let Some((algorithm, digest)) = parse_integrity_algorithm_and_digest(token) else {
        return ParsedIntegrityTokenResult {
            token: None,
            saw_supported_algorithm: false,
        };
    };
    let digest = digest.split_once('?').map_or(digest, |(digest, _)| digest);
    let expected_digest = decode_integrity_digest(digest);
    ParsedIntegrityTokenResult {
        token: expected_digest.map(|expected_digest| ParsedIntegrityToken {
            algorithm,
            expected_digest,
        }),
        saw_supported_algorithm: true,
    }
}

fn parse_integrity_algorithm_and_digest(
    token: &str,
) -> Option<(SubresourceIntegrityAlgorithm, &str)> {
    const PREFIXES: &[(&str, SubresourceIntegrityAlgorithm)] = &[
        ("sha256", SubresourceIntegrityAlgorithm::Sha256),
        ("sha-256", SubresourceIntegrityAlgorithm::Sha256),
        ("sha384", SubresourceIntegrityAlgorithm::Sha384),
        ("sha-384", SubresourceIntegrityAlgorithm::Sha384),
        ("sha512", SubresourceIntegrityAlgorithm::Sha512),
        ("sha-512", SubresourceIntegrityAlgorithm::Sha512),
    ];
    for (prefix, algorithm) in PREFIXES {
        let Some(rest) = token.strip_prefix(prefix) else {
            continue;
        };
        let Some(digest) = rest.strip_prefix('-') else {
            continue;
        };
        return Some((*algorithm, digest));
    }
    None
}

fn decode_integrity_digest(digest: &str) -> Option<Vec<u8>> {
    [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ]
    .into_iter()
    .find_map(|engine| engine.decode(digest).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observe(integrity: &str) {
        observe_subresource_integrity_metadata(Some(integrity));
    }

    #[test]
    fn script_integrity_metadata_parses_matching_supported_hash() {
        let body = b"console.log('integrity ok')";
        let digest = base64::engine::general_purpose::STANDARD
            .encode(DigestAlgorithm::Sha384.digest_bytes(body));
        let integrity = format!("sha384-{digest}");

        let metadata = parse_integrity_metadata(&integrity);
        assert!(metadata.saw_supported_algorithm);
        assert_eq!(metadata.tokens.len(), 1);
        assert_eq!(
            metadata.tokens[0].algorithm,
            SubresourceIntegrityAlgorithm::Sha384
        );
        assert_eq!(metadata.tokens[0].expected_digest.len(), 48);
        observe(&integrity);
    }

    #[test]
    fn script_integrity_metadata_accepts_unpadded_supported_hashes() {
        let body = b"console.log('unpadded integrity')";
        let sha256 = base64::engine::general_purpose::STANDARD_NO_PAD
            .encode(DigestAlgorithm::Sha256.digest_bytes(body));
        let sha512 = base64::engine::general_purpose::STANDARD_NO_PAD
            .encode(DigestAlgorithm::Sha512.digest_bytes(body));

        observe(&format!("sha256-{sha256}"));
        observe(&format!("sha512-{sha512}"));
    }

    #[test]
    fn script_integrity_metadata_accepts_chromium_algorithm_aliases() {
        let body = b"console.log('chromium algorithm aliases')";
        let digest = base64::engine::general_purpose::STANDARD
            .encode(DigestAlgorithm::Sha384.digest_bytes(body));

        let metadata = parse_integrity_metadata(&format!("sha-384-{digest}"));
        assert!(metadata.saw_supported_algorithm);
        assert_eq!(metadata.tokens.len(), 1);
    }

    #[test]
    fn script_integrity_metadata_accepts_base64url_hashes() {
        let body = b"console.log('base64url integrity')";
        let digest = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(DigestAlgorithm::Sha384.digest_bytes(body));

        observe(&format!("sha384-{digest}"));
    }

    #[test]
    fn script_integrity_metadata_ignores_unrecognized_algorithms() {
        let metadata = parse_integrity_metadata("sha1-this-is-ignored");
        assert!(!metadata.saw_supported_algorithm);
        assert!(metadata.tokens.is_empty());

        observe("sha1-this-is-ignored");
    }

    #[test]
    fn script_integrity_mismatch_is_non_blocking() {
        observe("sha384-doesnotmatch");
        observe("sha384-invalid");
    }

    #[test]
    fn script_integrity_metadata_tracks_strongest_supported_hash() {
        let body = b"console.log('strongest')";
        let weak = base64::engine::general_purpose::STANDARD
            .encode(DigestAlgorithm::Sha256.digest_bytes(b"wrong"));
        let strong = base64::engine::general_purpose::STANDARD
            .encode(DigestAlgorithm::Sha384.digest_bytes(body));
        let integrity = format!("sha256-{weak} sha384-{strong}");

        let strongest = parse_integrity_metadata(&integrity)
            .tokens
            .iter()
            .map(|token| token.algorithm)
            .max_by_key(|algorithm| algorithm.output_len_bytes());
        assert_eq!(strongest, Some(SubresourceIntegrityAlgorithm::Sha384));
        observe(&integrity);
    }

    #[test]
    fn script_integrity_wrong_length_stronger_hash_is_non_blocking() {
        let body = b"console.log('strongest wrong length')";
        let weak = base64::engine::general_purpose::STANDARD
            .encode(DigestAlgorithm::Sha384.digest_bytes(body));
        let integrity = format!("sha384-{weak} sha512-aW52YWxpZA==");

        observe(&integrity);
    }
}
