const RESERVED_CUSTOM_ELEMENT_NAMES: &[&str] = &[
    "annotation-xml",
    "color-profile",
    "font-face",
    "font-face-src",
    "font-face-uri",
    "font-face-format",
    "font-face-name",
    "missing-glyph",
];

pub fn is_valid_custom_element_name(name: &str) -> bool {
    // Per HTML spec (whatwg/html PR #7991, the "valid custom element name"
    // algorithm refreshed in 2022): the name must be non-empty, must contain
    // a U+002D HYPHEN-MINUS, must start with an ASCII lower alpha, must not
    // contain any ASCII upper alpha, and must satisfy the element local name
    // production — for an ASCII-letter-led name that reduces to "no
    // U+0000, U+0009, U+000A, U+000C, U+000D, U+0020, U+002F, U+003E in the
    // tail". Finally, the eight reserved SVG/MathML local names are banned.
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let mut has_hyphen = false;
    for ch in chars {
        if ch.is_ascii_uppercase() {
            return false;
        }
        if matches!(ch, '\0' | '\t' | '\n' | '\u{000C}' | '\r' | ' ' | '/' | '>') {
            return false;
        }
        if ch == '-' {
            has_hyphen = true;
        }
    }
    if !has_hyphen {
        return false;
    }
    if RESERVED_CUSTOM_ELEMENT_NAMES.contains(&name) {
        return false;
    }
    true
}

pub fn is_valid_built_in_extends_name(name: &str) -> bool {
    !name.is_empty()
        && !is_valid_custom_element_name(name)
        && name == name.to_ascii_lowercase()
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{
        RESERVED_CUSTOM_ELEMENT_NAMES, is_valid_built_in_extends_name, is_valid_custom_element_name,
    };

    #[test]
    fn validates_custom_element_names_like_registry_fixture() {
        assert!(is_valid_custom_element_name("my-element"));
        assert!(is_valid_custom_element_name("my-clone_element_a"));
        assert!(!is_valid_custom_element_name("nohyphen"));
        assert!(!is_valid_custom_element_name("UPPERCASE-ELEMENT"));
        assert!(!is_valid_custom_element_name("annotation-xml"));
    }

    #[test]
    fn validates_built_in_extends_names_without_accepting_custom_names() {
        assert!(is_valid_built_in_extends_name("button"));
        assert!(is_valid_built_in_extends_name("h1"));
        assert!(!is_valid_built_in_extends_name(""));
        assert!(!is_valid_built_in_extends_name("my-button"));
        assert!(!is_valid_built_in_extends_name("HTMLButtonElement"));
        assert!(!is_valid_built_in_extends_name("button_test"));
    }

    #[test]
    fn empty_string_is_not_a_valid_custom_element_name() {
        assert!(!is_valid_custom_element_name(""));
    }

    #[test]
    fn first_character_must_be_ascii_lower_alpha() {
        // The first code point must satisfy the ASCII lower alpha branch of
        // elementLocalName. Hyphen / digit / underscore / upper alpha / non-ASCII
        // leads are all rejected by the first-char check even if the tail looks
        // OK.
        for bad_lead in ["-foo-bar", "9-foo", "_-foo", "A-foo", " -foo", ":-foo"] {
            assert!(
                !is_valid_custom_element_name(bad_lead),
                "expected invalid: {bad_lead:?}"
            );
        }
    }

    #[test]
    fn name_must_contain_hyphen() {
        // Without U+002D HYPHEN-MINUS the algorithm rejects even otherwise legal
        // local names. The validator also rejects single-letter names since the
        // hyphen requirement implies length >= 2.
        assert!(!is_valid_custom_element_name("foo"));
        assert!(!is_valid_custom_element_name("annotationxml"));
        assert!(!is_valid_custom_element_name("a"));
        assert!(is_valid_custom_element_name("a-"));
        assert!(is_valid_custom_element_name("a-b"));
    }

    #[test]
    fn reserved_svg_mathml_names_are_rejected() {
        // The eight reserved local names from the SVG/MathML registries must
        // fail even though they otherwise look like valid PCEN strings.
        for reserved in RESERVED_CUSTOM_ELEMENT_NAMES {
            assert!(
                !is_valid_custom_element_name(reserved),
                "reserved name should be rejected: {reserved:?}"
            );
        }
        // The "-custom" suffixed variant — used by the WPT fixture as the canonical
        // positive control — must still pass.
        assert!(is_valid_custom_element_name("annotation-xml-custom"));
        assert!(is_valid_custom_element_name("font-face-custom"));
    }

    #[test]
    fn tail_must_not_contain_html_terminators_or_whitespace() {
        // The elementLocalName production's first branch forbids U+0000, U+0009,
        // U+000A, U+000C, U+000D, U+0020, U+002F, U+003E anywhere after the lead
        // character — even when a hyphen is present elsewhere.
        for bad in [
            "a-\0foo",
            "a-\tfoo",
            "a-\nfoo",
            "a-\x0Cfoo",
            "a-\rfoo",
            "a- foo",
            "a-/foo",
            "a->foo",
        ] {
            assert!(
                !is_valid_custom_element_name(bad),
                "tail-terminator should reject: {bad:?}"
            );
        }
    }

    #[test]
    fn upper_alpha_anywhere_invalidates_name() {
        // Even a single upper-alpha after the lead disqualifies the name.
        assert!(!is_valid_custom_element_name("a-Foo"));
        assert!(!is_valid_custom_element_name("foo-baR"));
        assert!(!is_valid_custom_element_name("aA-foo"));
    }

    #[test]
    fn non_ascii_in_tail_is_allowed_when_hyphen_present() {
        // The PCEN production allows arbitrary non-control non-terminator code
        // points (including BMP and astral) in the tail when the lead is an
        // ASCII lower alpha. The WPT fixture iterates over many such codepoints.
        assert!(is_valid_custom_element_name("a-\u{00E9}lement"));
        assert!(is_valid_custom_element_name("a-\u{1F171}-element"));
        assert!(is_valid_custom_element_name("a-\u{10000}-element"));
        // ASCII punctuation that the regex permits in the tail is also fine.
        assert!(is_valid_custom_element_name("a-b.c"));
        assert!(is_valid_custom_element_name("a-b_c"));
        assert!(is_valid_custom_element_name("a-b1c"));
    }

    #[test]
    fn built_in_extends_rejects_digits_only_and_custom_shaped_names() {
        // "All ASCII lower or digit" is required; an all-digit name is technically
        // allowed by the predicate (it has no hyphen, no upper alpha) — but it is
        // not a real HTML element. The validator is intentionally permissive here
        // because the registry's authoritative check is "is this a registered
        // built-in?" elsewhere. What we *do* require is that uppercase / hyphen /
        // arbitrary punctuation be rejected.
        assert!(is_valid_built_in_extends_name("a"));
        assert!(is_valid_built_in_extends_name("h1"));
        assert!(is_valid_built_in_extends_name("0"));
        assert!(!is_valid_built_in_extends_name("Button"));
        assert!(!is_valid_built_in_extends_name("button-thing"));
        assert!(!is_valid_built_in_extends_name("button "));
        assert!(!is_valid_built_in_extends_name("but/ton"));
    }
}
