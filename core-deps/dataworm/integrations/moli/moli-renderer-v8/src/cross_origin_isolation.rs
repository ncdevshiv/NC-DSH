use url::Url;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CrossOriginEmbedderPolicy {
    #[default]
    None,
    RequireCorp,
    Credentialless,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DocumentIsolationPolicy {
    #[default]
    None,
    IsolateAndRequireCorp,
    IsolateAndCredentialless,
}

pub(crate) fn response_headers_enable_cross_origin_isolation(
    final_url: &Url,
    headers: &[(String, String)],
) -> bool {
    if !moli_url::is_potentially_trustworthy_url(final_url) {
        return false;
    }
    if document_isolation_policy_from_headers(headers).enables_cross_origin_isolation() {
        return true;
    }
    let coop = response_header_policy_value(headers, "cross-origin-opener-policy");
    matches!(coop.as_deref(), Some("same-origin"))
        && cross_origin_embedder_policy_from_headers(headers).enables_cross_origin_isolation()
}

pub(crate) fn cross_origin_embedder_policy_from_headers(
    headers: &[(String, String)],
) -> CrossOriginEmbedderPolicy {
    match response_header_policy_value(headers, "cross-origin-embedder-policy").as_deref() {
        Some("require-corp") => CrossOriginEmbedderPolicy::RequireCorp,
        Some("credentialless") => CrossOriginEmbedderPolicy::Credentialless,
        _ => CrossOriginEmbedderPolicy::None,
    }
}

pub(crate) fn document_isolation_policy_from_headers(
    headers: &[(String, String)],
) -> DocumentIsolationPolicy {
    match response_header_policy_value(headers, "document-isolation-policy").as_deref() {
        Some("isolate-and-require-corp") => DocumentIsolationPolicy::IsolateAndRequireCorp,
        Some("isolate-and-credentialless") => DocumentIsolationPolicy::IsolateAndCredentialless,
        _ => DocumentIsolationPolicy::None,
    }
}

impl CrossOriginEmbedderPolicy {
    pub(crate) fn enables_cross_origin_isolation(self) -> bool {
        matches!(self, Self::RequireCorp | Self::Credentialless)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::RequireCorp => "require-corp",
            Self::Credentialless => "credentialless",
        }
    }
}

impl DocumentIsolationPolicy {
    pub(crate) fn enables_cross_origin_isolation(self) -> bool {
        matches!(
            self,
            Self::IsolateAndRequireCorp | Self::IsolateAndCredentialless
        )
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::IsolateAndRequireCorp => "isolate-and-require-corp",
            Self::IsolateAndCredentialless => "isolate-and-credentialless",
        }
    }
}

fn response_header_policy_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .rev()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coop_coep_headers_enable_cross_origin_isolation_for_trustworthy_urls() {
        let url = Url::parse("https://example.test/").expect("valid url");
        let headers = vec![
            (
                "Cross-Origin-Embedder-Policy".to_owned(),
                "require-corp".to_owned(),
            ),
            (
                "Cross-Origin-Opener-Policy".to_owned(),
                "same-origin".to_owned(),
            ),
        ];
        assert!(response_headers_enable_cross_origin_isolation(
            &url, &headers
        ));
    }

    #[test]
    fn cross_origin_isolation_requires_both_headers() {
        let url = Url::parse("https://example.test/").expect("valid url");
        let headers = vec![(
            "Cross-Origin-Opener-Policy".to_owned(),
            "same-origin".to_owned(),
        )];
        assert!(!response_headers_enable_cross_origin_isolation(
            &url, &headers
        ));
    }

    #[test]
    fn document_isolation_policy_enables_cross_origin_isolation_for_trustworthy_urls() {
        let url = Url::parse("https://example.test/").expect("valid url");
        let headers = vec![(
            "Document-Isolation-Policy".to_owned(),
            "isolate-and-require-corp".to_owned(),
        )];
        assert!(response_headers_enable_cross_origin_isolation(
            &url, &headers
        ));
    }

    #[test]
    fn document_isolation_policy_cross_origin_isolation_requires_trustworthy_url() {
        let url = Url::parse("http://example.test/").expect("valid url");
        let headers = vec![(
            "Document-Isolation-Policy".to_owned(),
            "isolate-and-credentialless".to_owned(),
        )];
        assert!(!response_headers_enable_cross_origin_isolation(
            &url, &headers
        ));
    }

    #[test]
    fn parses_cross_origin_embedder_policy_header_values() {
        assert_eq!(
            cross_origin_embedder_policy_from_headers(&[(
                "Cross-Origin-Embedder-Policy".to_owned(),
                "require-corp; report-to=\"endpoint\"".to_owned()
            )]),
            CrossOriginEmbedderPolicy::RequireCorp
        );
        assert_eq!(
            cross_origin_embedder_policy_from_headers(&[(
                "Cross-Origin-Embedder-Policy".to_owned(),
                "credentialless".to_owned()
            )]),
            CrossOriginEmbedderPolicy::Credentialless
        );
        assert_eq!(
            cross_origin_embedder_policy_from_headers(&[(
                "Cross-Origin-Embedder-Policy".to_owned(),
                "invalid".to_owned()
            )]),
            CrossOriginEmbedderPolicy::None
        );
    }

    #[test]
    fn parses_document_isolation_policy_header_values() {
        assert_eq!(
            document_isolation_policy_from_headers(&[(
                "Document-Isolation-Policy".to_owned(),
                "isolate-and-require-corp; report-to=\"endpoint\"".to_owned()
            )]),
            DocumentIsolationPolicy::IsolateAndRequireCorp
        );
        assert_eq!(
            document_isolation_policy_from_headers(&[(
                "Document-Isolation-Policy".to_owned(),
                "isolate-and-credentialless".to_owned()
            )]),
            DocumentIsolationPolicy::IsolateAndCredentialless
        );
        assert_eq!(
            document_isolation_policy_from_headers(&[(
                "Document-Isolation-Policy".to_owned(),
                "invalid".to_owned()
            )]),
            DocumentIsolationPolicy::None
        );
    }
}
