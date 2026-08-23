//! Property-based tests for secret redaction.
//!
//! The non-negotiable guarantees: redaction is idempotent, registered
//! secrets can never survive into output, and key/bearer-shaped tokens are
//! masked even when no secret was registered.

#![cfg(test)]

use proptest::prelude::*;

use crate::Redactor;

proptest! {
    /// Redaction is idempotent and fully removes registered secrets.
    #[test]
    fn redaction_is_idempotent_and_hides_secrets(
        secret in "[A-Za-z0-9]{8,24}",
        text in ".*",
    ) {
        let redactor = Redactor::new(vec![secret.clone()]);
        let once = redactor.redact(&text);
        let twice = redactor.redact(&once);
        prop_assert_eq!(&once, &twice);
        prop_assert!(!once.contains(&secret), "secret leaked: {once}");
    }

    /// API-key-shaped tokens (`sk-` + 12+ alphanumerics) are masked even
    /// without registered secrets.
    #[test]
    fn api_key_shaped_tokens_are_masked(key in "[a-zA-Z0-9]{12,40}") {
        let needle = format!("sk-{key}");
        let redacted = Redactor::new(vec![]).redact(&format!("The key is {needle}, keep it safe"));
        prop_assert!(!redacted.contains(&needle), "leftover: {redacted}");
    }

    /// Bearer tokens are masked.
    #[test]
    fn bearer_tokens_are_masked(token in "[a-zA-Z0-9._-]{12,32}") {
        let redacted =
            Redactor::new(vec![]).redact(&format!("Authorization: Bearer {token}"));
        prop_assert!(!redacted.contains(&token), "leftover: {redacted}");
    }
}
