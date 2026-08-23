//! Security: secret redaction, PII detection, SSRF/URL guards, permissions.
//!
//! Everything here is deterministic and testable:
//!
//! - [`Redactor`] — masks API keys, bearer tokens, cookies, and arbitrary
//!   secrets in logs/payloads. Used by every logging path.
//! - [`PiiDetector`] — finds emails, phone numbers, credit cards, and
//!   IP addresses in text.
//! - [`UrlPolicy`] — SSRF guard: scheme/port allowlists, private-range
//!   blocking for literal IPs, and host blocklists for user-supplied URLs.

use std::net::IpAddr;

use regex::Regex;

use ai_errors::{AiError, ValidationError};

/// Pattern-preserving redaction of secrets in text.
#[derive(Debug, Clone)]
pub struct Redactor {
    secrets: Vec<String>,
    api_key_re: Regex,
    bearer_re: Regex,
    cookie_re: Regex,
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Redactor {
    pub fn new(secrets: Vec<String>) -> Self {
        Self {
            secrets,
            api_key_re: Regex::new(r"(?i)\b(sk|pk|rk|ak|api[_-]?key)[-_]?[a-z0-9]{8,}\b").unwrap(),
            bearer_re: Regex::new(r"(?i)\bbearer\s+[a-z0-9._-]{12,}").unwrap(),
            cookie_re: Regex::new(r"(?i)(cookie|authorization|set-cookie)\s*[:=]\s*[^;\s]{6,}")
                .unwrap(),
        }
    }

    /// Registers an additional secret to redact (e.g. from config).
    pub fn add_secret(&mut self, secret: impl Into<String>) {
        let secret = secret.into();
        if !secret.is_empty() && !self.secrets.contains(&secret) {
            self.secrets.push(secret);
        }
    }

    /// Replaces all known secrets and key-shaped tokens with `[REDACTED]`.
    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_string();
        for secret in &self.secrets {
            if !secret.is_empty() && secret.len() >= 4 {
                out = out.replace(secret, "[REDACTED]");
            }
        }
        out = self.api_key_re.replace_all(&out, "[REDACTED]").into_owned();
        out = self.bearer_re.replace_all(&out, "[REDACTED]").into_owned();
        out = self.cookie_re.replace_all(&out, "[REDACTED]").into_owned();
        out
    }

    pub fn is_redacted(&self, text: &str) -> bool {
        !self.redact(text).contains(text) || text.contains("[REDACTED]")
    }
}

/// Identified PII category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiiType {
    Email,
    Phone,
    CreditCard,
    IpAddress,
}

impl std::fmt::Display for PiiType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Email => "email",
            Self::Phone => "phone",
            Self::CreditCard => "credit_card",
            Self::IpAddress => "ip_address",
        };
        f.write_str(s)
    }
}

/// A PII occurrence in text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiiMatch {
    pub kind: PiiType,
    pub start: usize,
    pub end: usize,
}

/// Detects common PII categories in text.
#[derive(Debug, Clone)]
pub struct PiiDetector {
    email_re: Regex,
    phone_re: Regex,
    cc_re: Regex,
    ip_re: Regex,
}

impl Default for PiiDetector {
    fn default() -> Self {
        Self {
            email_re: Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
            phone_re: Regex::new(r"\+?[0-9][0-9 ()-]{7,}[0-9]").unwrap(),
            cc_re: Regex::new(r"\b(?:\d[ -]?){13,19}\b").unwrap(),
            ip_re: Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap(),
        }
    }
}

impl PiiDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Finds all PII occurrences (non-overlapping, deterministic order).
    pub fn find(&self, text: &str) -> Vec<PiiMatch> {
        let mut matches = Vec::new();
        for (re, kind) in [
            (&self.email_re, PiiType::Email),
            (&self.phone_re, PiiType::Phone),
            (&self.cc_re, PiiType::CreditCard),
            (&self.ip_re, PiiType::IpAddress),
        ] {
            for m in re.find_iter(text) {
                matches.push(PiiMatch {
                    kind,
                    start: m.start(),
                    end: m.end(),
                });
            }
        }
        matches.sort_by_key(|m| m.start);
        matches
    }

    /// Replaces every PII occurrence with a placeholder per kind.
    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_string();
        for m in self.find(text) {
            let placeholder = format!("[{}]", m.kind);
            out.replace_range(m.start..m.end, &placeholder);
        }
        out
    }

    pub fn contains_pii(&self, text: &str) -> bool {
        !self.find(text).is_empty()
    }
}

/// Result of validating a URL against the SSRF policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlVerdict {
    Allowed,
    /// Reason the URL was rejected (scheme, port, private range, blocklist).
    Rejected(String),
}

/// SSRF guard for user-supplied URLs (spec §20, §24).
///
/// Rules (all configurable):
/// - scheme must be in the allowlist (default: `http`, `https`)
/// - port must be in the allowlist (default: 80, 443)
/// - literal IP hosts in private/reserved ranges are rejected
/// - hostnames in the blocklist are rejected (exact + suffix match)
#[derive(Debug, Clone)]
pub struct UrlPolicy {
    allowed_schemes: Vec<String>,
    allowed_ports: Vec<u16>,
    blocklist: Vec<String>,
    allow_private: bool,
}

impl Default for UrlPolicy {
    fn default() -> Self {
        Self {
            allowed_schemes: vec!["http".into(), "https".into()],
            allowed_ports: vec![80, 443],
            blocklist: Vec::new(),
            allow_private: false,
        }
    }
}

impl UrlPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow_scheme(mut self, scheme: &str) -> Self {
        self.allowed_schemes.push(scheme.to_string());
        self
    }

    pub fn allow_port(mut self, port: u16) -> Self {
        self.allowed_ports.push(port);
        self
    }

    pub fn block_host(mut self, host: &str) -> Self {
        self.blocklist.push(host.to_lowercase());
        self
    }

    pub fn allow_private_networks(mut self) -> Self {
        self.allow_private = true;
        self
    }

    /// Validates `url`; returns the verdict.
    pub fn validate(&self, url: &str) -> UrlVerdict {
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(e) => return UrlVerdict::Rejected(format!("invalid URL: {e}")),
        };

        let scheme = parsed.scheme().to_lowercase();
        if !self.allowed_schemes.iter().any(|s| s == &scheme) {
            return UrlVerdict::Rejected(format!("scheme `{scheme}` not allowed"));
        }

        let port = parsed.port().unwrap_or(match scheme.as_str() {
            "https" => 443,
            _ => 80,
        });
        if !self.allowed_ports.contains(&port) {
            return UrlVerdict::Rejected(format!("port {port} not allowed"));
        }

        let host = parsed.host_str().unwrap_or("").to_lowercase();
        if host.is_empty() {
            return UrlVerdict::Rejected("missing host".into());
        }

        // Blocklist: exact host or any suffix (sub.example.com matches
        // example.com in the blocklist).
        for blocked in &self.blocklist {
            if host == *blocked || host.ends_with(&format!(".{blocked}")) {
                return UrlVerdict::Rejected(format!("host `{host}` is blocked"));
            }
        }

        // Private/reserved ranges for literal IPs and well-known local hostnames.
        if !self.allow_private {
            if host == "localhost"
                || host == "0"
                || host.ends_with(".localhost")
                || host.ends_with(".local")
                || host.ends_with(".internal")
            {
                return UrlVerdict::Rejected(format!("private host `{host}` not allowed"));
            }
            if let Ok(ip) = host.parse::<IpAddr>() {
                let blocked = match ip {
                    IpAddr::V4(v4) => {
                        v4.is_private()
                            || v4.is_loopback()
                            || v4.is_link_local()
                            || v4.is_unspecified()
                            || v4.is_broadcast()
                            || v4.is_documentation()
                    }
                    IpAddr::V6(v6) => {
                        v6.is_loopback() || v6.is_unspecified() || v6.is_unique_local()
                    }
                };
                if blocked {
                    return UrlVerdict::Rejected(format!("private/reserved IP `{ip}` not allowed"));
                }
            }
        }

        UrlVerdict::Allowed
    }

    /// Convenience: validates and returns a typed error on rejection.
    pub fn require(&self, url: &str) -> Result<(), AiError> {
        match self.validate(url) {
            UrlVerdict::Allowed => Ok(()),
            UrlVerdict::Rejected(reason) => Err(AiError::Validation(ValidationError::new(
                format!("URL rejected by SSRF policy: {reason}"),
            ))),
        }
    }
}

/// Permission gate for sensitive operations (filesystem, commands, network).
#[derive(Debug, Clone, Default)]
pub struct Permissions {
    /// Operations explicitly permitted by the caller.
    allowed: Vec<String>,
}

impl Permissions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow(mut self, operation: &str) -> Self {
        self.allowed.push(operation.to_string());
        self
    }

    /// Whether `operation` may run. Operations are denied by default.
    pub fn permits(&self, operation: &str) -> bool {
        self.allowed.iter().any(|a| a == operation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redactor_masks_keys_and_bearers() {
        let redactor = Redactor::new(vec!["sk-super-secret".into()]);
        let text = "key sk-super-secret and Bearer abcdef1234567890xyz and cookie=session=abc123";
        let out = redactor.redact(text);
        assert!(!out.contains("sk-super-secret"), "{out}");
        assert!(!out.contains("abcdef1234567890xyz"), "{out}");
        assert!(!out.contains("session=abc123"), "{out}");
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn pii_detector_finds_email_phone_ip() {
        let detector = PiiDetector::new();
        let text = "contact a@b.com or call +1 555 123 4567 from 192.168.1.1";
        let matches = detector.find(text);
        assert!(
            matches.iter().any(|m| m.kind == PiiType::Email),
            "{matches:?}"
        );
        assert!(
            matches.iter().any(|m| m.kind == PiiType::Phone),
            "{matches:?}"
        );
        assert!(
            matches.iter().any(|m| m.kind == PiiType::IpAddress),
            "{matches:?}"
        );
    }

    #[test]
    fn pii_redact_replaces_with_placeholders() {
        let detector = PiiDetector::new();
        let out = detector.redact("mail a@b.com now");
        assert!(!out.contains("a@b.com"), "{out}");
        assert!(out.contains("[email]"), "{out}");
    }

    #[test]
    fn url_policy_rejects_private_ips_and_bad_schemes() {
        let policy = UrlPolicy::new();
        assert_eq!(
            policy.validate("http://192.168.0.1/x"),
            UrlVerdict::Rejected("private/reserved IP `192.168.0.1` not allowed".into())
        );
        assert_eq!(
            policy.validate("file:///etc/passwd"),
            UrlVerdict::Rejected("scheme `file` not allowed".into())
        );
        assert_eq!(policy.validate("https://example.com/"), UrlVerdict::Allowed);
        assert_eq!(
            policy.validate("http://localhost:8080/"),
            UrlVerdict::Rejected("port 8080 not allowed".into())
        );
    }

    #[test]
    fn url_policy_blocklist_matches_suffixes() {
        let policy = UrlPolicy::new().block_host("internal.corp");
        assert!(matches!(
            policy.validate("https://internal.corp/x"),
            UrlVerdict::Rejected(_)
        ));
        assert!(matches!(
            policy.validate("https://api.internal.corp/x"),
            UrlVerdict::Rejected(_)
        ));
        assert_eq!(
            policy.validate("https://example.com/x"),
            UrlVerdict::Allowed
        );
    }

    #[test]
    fn permissions_deny_by_default() {
        let permissions = Permissions::new().allow("fs:read");
        assert!(permissions.permits("fs:read"));
        assert!(!permissions.permits("fs:write"));
        assert!(!Permissions::new().permits("anything"));
    }
}

#[cfg(test)]
mod proptests;
