use moli_curl::CurlDnsResolution;
use moli_dns_resolver::DnsTarget;
use url::{Host, Url};

use crate::FetchConfig;

/// Fetch-side DNS admission decision.
///
/// The shared resolver is used only when Fetch can prove that curl will
/// connect directly to an HTTP(S) origin. Proxy traffic stays curl-managed:
/// the proxy, rather than the local process, may be responsible for resolving
/// the origin hostname. IP literals and explicit host-resolve configuration
/// already have exact routing and must not be resolved again.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FetchCurlDnsAdmission {
    CurlManaged,
    SharedResolver(DnsTarget),
}

pub(crate) fn curl_dns_resolution(config: &FetchConfig, url: &Url) -> CurlDnsResolution {
    match curl_dns_admission_with_env(config, url, |name| std::env::var(name).ok()) {
        FetchCurlDnsAdmission::CurlManaged => CurlDnsResolution::curl_managed(),
        FetchCurlDnsAdmission::SharedResolver(target) => {
            CurlDnsResolution::resolve_origin(target, config.http_host_resolve().to_vec())
        }
    }
}

fn curl_dns_admission_with_env(
    config: &FetchConfig,
    url: &Url,
    mut env: impl FnMut(&str) -> Option<String>,
) -> FetchCurlDnsAdmission {
    if !matches!(url.scheme(), "http" | "https") || !config.http_host_resolve().is_empty() {
        return FetchCurlDnsAdmission::CurlManaged;
    }
    let Some(Host::Domain(host)) = url.host() else {
        return FetchCurlDnsAdmission::CurlManaged;
    };
    let Some(port) = url.port_or_known_default() else {
        return FetchCurlDnsAdmission::CurlManaged;
    };
    if request_uses_proxy(config, url, host, port, &mut env) {
        return FetchCurlDnsAdmission::CurlManaged;
    }
    FetchCurlDnsAdmission::SharedResolver(DnsTarget::new(host, port))
}

fn request_uses_proxy(
    config: &FetchConfig,
    url: &Url,
    host: &str,
    port: u16,
    env: &mut impl FnMut(&str) -> Option<String>,
) -> bool {
    let proxy = match config.http_proxy() {
        Some("") => return false,
        Some(proxy) => Some(proxy.to_owned()),
        None => env_proxy_for_scheme(url.scheme(), env),
    };
    let Some(_) = proxy.filter(|proxy| !proxy.is_empty()) else {
        return false;
    };
    let no_proxy = match config.http_no_proxy() {
        Some(no_proxy) => Some(no_proxy.to_owned()),
        None => env_no_proxy(env),
    };
    !no_proxy_matches(host, port, no_proxy.as_deref())
}

fn env_proxy_for_scheme(
    scheme: &str,
    env: &mut impl FnMut(&str) -> Option<String>,
) -> Option<String> {
    let names: &[&str] = match scheme {
        // curl deliberately ignores uppercase HTTP_PROXY because CGI servers
        // commonly expose an attacker-controlled Proxy header under that name.
        "http" => &["http_proxy"],
        "https" => &["https_proxy", "HTTPS_PROXY"],
        _ => &[],
    };
    for name in names {
        if let Some(value) = env(name).filter(|value| !value.is_empty()) {
            return Some(value);
        }
    }
    for name in ["all_proxy", "ALL_PROXY"] {
        if let Some(value) = env(name).filter(|value| !value.is_empty()) {
            return Some(value);
        }
    }
    None
}

fn env_no_proxy(env: &mut impl FnMut(&str) -> Option<String>) -> Option<String> {
    env("no_proxy")
        .filter(|value| !value.is_empty())
        .or_else(|| env("NO_PROXY").filter(|value| !value.is_empty()))
}

fn no_proxy_matches(host: &str, port: u16, no_proxy: Option<&str>) -> bool {
    let Some(no_proxy) = no_proxy else {
        return false;
    };
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    no_proxy.split(',').any(|token| {
        let token = token.trim();
        if token.is_empty() {
            return false;
        }
        if token == "*" {
            return true;
        }
        let (token_host, token_port) = split_no_proxy_host_port(token);
        if let Some(token_port) = token_port
            && token_port != port
        {
            return false;
        }
        let token_host = token_host
            .trim_matches(['[', ']'])
            .trim_start_matches('.')
            .to_ascii_lowercase();
        !token_host.is_empty()
            && (host == token_host
                || host
                    .strip_suffix(&token_host)
                    .is_some_and(|prefix| prefix.ends_with('.')))
    })
}

fn split_no_proxy_host_port(token: &str) -> (&str, Option<u16>) {
    let Some((host, port)) = token.rsplit_once(':') else {
        return (token, None);
    };
    match port.parse::<u16>() {
        Ok(port) if !host.contains(':') => (host, Some(port)),
        _ => (token, None),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn admission(
        config: &FetchConfig,
        raw_url: &str,
        env: &[(&str, &str)],
    ) -> FetchCurlDnsAdmission {
        let env = env
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<HashMap<_, _>>();
        curl_dns_admission_with_env(
            config,
            &Url::parse(raw_url).expect("test URL should parse"),
            |name| env.get(name).cloned(),
        )
    }

    fn shared_target(host: &str, port: u16) -> FetchCurlDnsAdmission {
        FetchCurlDnsAdmission::SharedResolver(DnsTarget::new(host, port))
    }

    #[test]
    fn direct_http_and_https_domains_use_shared_resolution() {
        let config = FetchConfig::default();

        assert_eq!(
            admission(&config, "http://example.test/path", &[]),
            shared_target("example.test", 80)
        );
        assert_eq!(
            admission(&config, "https://example.test:8443/path", &[]),
            shared_target("example.test", 8443)
        );
    }

    #[test]
    fn ip_literals_and_host_resolve_configuration_stay_curl_managed() {
        let mut config = FetchConfig::default();

        assert_eq!(
            admission(&config, "http://127.0.0.1/path", &[]),
            FetchCurlDnsAdmission::CurlManaged
        );
        assert_eq!(
            admission(&config, "http://[::1]/path", &[]),
            FetchCurlDnsAdmission::CurlManaged
        );
        config.set_http_host_resolve(vec!["example.test:80:127.0.0.1".to_owned()]);
        assert_eq!(
            admission(&config, "http://example.test/path", &[]),
            FetchCurlDnsAdmission::CurlManaged
        );
    }

    #[test]
    fn explicit_proxy_uses_curl_resolution_unless_no_proxy_matches() {
        let mut config = FetchConfig::default();
        config.set_http_proxy(Some("http://proxy.test:8080".to_owned()));

        assert_eq!(
            admission(&config, "https://api.example.test/path", &[]),
            FetchCurlDnsAdmission::CurlManaged
        );
        config.set_http_no_proxy(Some(".example.test".to_owned()));
        assert_eq!(
            admission(&config, "https://api.example.test/path", &[]),
            shared_target("api.example.test", 443)
        );
    }

    #[test]
    fn empty_explicit_proxy_disables_environment_proxy_fallback() {
        let mut config = FetchConfig::default();
        config.set_http_proxy(Some(String::new()));

        assert_eq!(
            admission(
                &config,
                "http://example.test/path",
                &[("http_proxy", "http://proxy.test:8080")],
            ),
            shared_target("example.test", 80)
        );
    }

    #[test]
    fn environment_proxy_and_no_proxy_follow_curl_precedence() {
        let config = FetchConfig::default();

        assert_eq!(
            admission(
                &config,
                "http://example.test/path",
                &[("http_proxy", "http://proxy.test:8080")],
            ),
            FetchCurlDnsAdmission::CurlManaged
        );
        assert_eq!(
            admission(
                &config,
                "https://example.test/path",
                &[("HTTPS_PROXY", "http://proxy.test:8080")],
            ),
            FetchCurlDnsAdmission::CurlManaged
        );
        assert_eq!(
            admission(
                &config,
                "https://api.example.test/path",
                &[
                    ("all_proxy", "http://proxy.test:8080"),
                    ("NO_PROXY", "example.test"),
                ],
            ),
            shared_target("api.example.test", 443)
        );
    }

    #[test]
    fn no_proxy_port_and_domain_boundaries_are_exact() {
        assert!(no_proxy_matches(
            "api.example.test",
            8443,
            Some("example.test:8443")
        ));
        assert!(!no_proxy_matches(
            "api.example.test",
            443,
            Some("example.test:8443")
        ));
        assert!(!no_proxy_matches(
            "notexample.test",
            443,
            Some("example.test")
        ));
        assert!(no_proxy_matches("anything.test", 443, Some("*")));
    }
}
