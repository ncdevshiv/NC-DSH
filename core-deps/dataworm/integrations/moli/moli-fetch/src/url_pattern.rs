use wildcard::Wildcard;

fn normalize_url_pattern_for_wildcard(pattern: &str) -> Vec<u8> {
    let pattern = pattern.as_bytes();
    let mut normalized = Vec::with_capacity(pattern.len());
    let mut index = 0usize;
    while index < pattern.len() {
        match pattern[index] {
            b'\\' => {
                if let Some(next) = pattern.get(index + 1).copied() {
                    if matches!(next, b'*' | b'?' | b'\\') {
                        normalized.push(b'\\');
                    }
                    normalized.push(next);
                    index += 2;
                } else {
                    normalized.extend_from_slice(br"\\");
                    index += 1;
                }
            }
            byte => {
                normalized.push(byte);
                index += 1;
            }
        }
    }
    normalized
}

/// Matches a DevTools-style URL pattern against a URL string.
///
/// This intentionally supports the small wildcard surface used by CDP Fetch
/// and Network blocking patterns: `*` matches any sequence, `?` matches one
/// byte, and `\` escapes the following byte.
pub fn url_pattern_matches(pattern: &str, value: &str) -> bool {
    let normalized = normalize_url_pattern_for_wildcard(pattern);
    Wildcard::new(&normalized).is_ok_and(|wildcard| wildcard.is_match(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::url_pattern_matches;

    #[test]
    fn url_pattern_matches_wildcards_and_escaped_literals() {
        assert!(url_pattern_matches(
            "https://example.com/*/api?x=*",
            "https://example.com/v1/api?x=1"
        ));
        assert!(!url_pattern_matches(
            "https://example.com/*/api?x=*",
            "https://example.com/v1/api"
        ));
        assert!(url_pattern_matches(
            r"https://example.com/file\*name",
            "https://example.com/file*name"
        ));
        assert!(!url_pattern_matches(
            r"https://example.com/file\*name",
            "https://example.com/filename"
        ));
    }

    #[test]
    fn url_pattern_matches_preserves_legacy_lenient_escapes() {
        assert!(url_pattern_matches(
            r"https://example.com/file\name",
            "https://example.com/filename"
        ));
        assert!(!url_pattern_matches(
            r"https://example.com/file\name",
            r"https://example.com/file\name"
        ));
        assert!(url_pattern_matches(
            r"https://example.com/path\\name",
            r"https://example.com/path\name"
        ));
        assert!(url_pattern_matches(
            r"https://example.com/path\",
            r"https://example.com/path\"
        ));
    }
}
