use super::{
    host_is_public_suffix, public_suffix_list, registrable_site_host, same_site_hosts,
    same_site_urls, schemeful_site_for_url, site_key_for_host,
};
use std::sync::Arc;
use url::Url;

#[test]
fn registrable_site_host_uses_last_registrable_suffix() {
    assert_eq!(registrable_site_host("sub.example.com"), "example.com");
    assert_eq!(registrable_site_host("a.b.example.co.uk"), "example.co.uk");
}

#[test]
fn same_site_hosts_treat_sibling_subdomains_as_same_site() {
    assert!(same_site_hosts("a.example.com", "b.example.com"));
    assert!(same_site_hosts(
        "img.shop.example.co.uk",
        "api.example.co.uk"
    ));
}

#[test]
fn same_site_hosts_respect_public_suffix_boundaries() {
    assert!(!same_site_hosts("foo.co.uk", "bar.co.uk"));
    assert!(!same_site_hosts("foo.github.io", "bar.github.io"));
}

#[test]
fn same_site_hosts_require_exact_match_for_ip_addresses() {
    assert!(same_site_hosts("127.0.0.1", "127.0.0.1"));
    assert!(!same_site_hosts("127.0.0.1", "127.0.0.2"));
}

#[test]
fn same_site_urls_can_require_scheme_match() {
    let https = Url::parse("https://app.example.com/index.html").unwrap();
    let http = Url::parse("http://api.example.com/frame.html").unwrap();

    assert!(same_site_urls(&https, &http, false));
    assert!(!same_site_urls(&https, &http, true));
}

#[test]
fn same_site_urls_treat_websocket_schemes_as_http_families() {
    let https = Url::parse("https://app.example.com/index.html").unwrap();
    let wss = Url::parse("wss://api.example.com/socket").unwrap();
    let http = Url::parse("http://app.example.com/index.html").unwrap();
    let ws = Url::parse("ws://api.example.com/socket").unwrap();

    assert!(same_site_urls(&https, &wss, true));
    assert!(same_site_urls(&http, &ws, true));
    assert!(!same_site_urls(&https, &ws, true));
    assert!(!same_site_urls(&http, &wss, true));
}

#[test]
fn same_site_urls_support_blob_urls_like_chromium_site_for_cookies() {
    let secure_blob =
        Url::parse("blob:https://example.org/9115d58c-bcda-ff47-86e5-083e9a2153041").unwrap();
    let secure_subresource = Url::parse("https://sub.example.org/resource").unwrap();
    let insecure_subresource = Url::parse("http://sub.example.org/resource").unwrap();

    assert!(same_site_urls(&secure_blob, &secure_subresource, true));
    assert!(!same_site_urls(&secure_blob, &insecure_subresource, true));
    assert!(same_site_urls(&secure_blob, &insecure_subresource, false));
}

#[test]
fn same_site_urls_support_blob_file_urls_like_chromium_site_for_cookies() {
    let local_blob = Url::parse("blob:file:///C:/app/index.html").unwrap();
    let local_file = Url::parse("file:///etc/shadow").unwrap();
    let nonlocal_file = Url::parse("file://nonlocal/file.txt").unwrap();

    assert!(same_site_urls(&local_blob, &local_file, true));
    assert!(!same_site_urls(&local_blob, &nonlocal_file, true));
}

#[test]
fn same_site_urls_treat_local_file_urls_as_same_site() {
    let local_a = Url::parse("file:///a/b/c").unwrap();
    let local_b = Url::parse("file:///etc/shadow").unwrap();
    let nonlocal = Url::parse("file://nonlocal/file.txt").unwrap();

    assert!(same_site_urls(&local_a, &local_b, true));
    assert!(!same_site_urls(&local_a, &nonlocal, true));
}

#[test]
fn same_site_urls_treat_nonstandard_schemes_as_match_none_like_chromium() {
    let first = Url::parse("non-standard://abc").unwrap();
    let second = Url::parse("non-standard://abc").unwrap();
    let third = Url::parse("non-standard://def").unwrap();

    assert!(!same_site_urls(&first, &second, false));
    assert!(!same_site_urls(&first, &third, false));
}

#[test]
fn site_key_for_host_normalizes_to_registrable_site() {
    assert_eq!(
        site_key_for_host("sub.example.com"),
        Some("example.com".into())
    );
    assert_eq!(
        site_key_for_host(".deep.example.co.uk"),
        Some("example.co.uk".into())
    );
    assert_eq!(site_key_for_host("127.0.0.1"), Some("127.0.0.1".into()));
    assert_eq!(site_key_for_host(""), None);
}

#[test]
fn schemeful_site_uses_registrable_domain_for_network_hosts() {
    assert_eq!(
        schemeful_site_for_url(&Url::parse("https://a.example.com/path").unwrap()),
        "https://example.com"
    );
    assert_eq!(
        schemeful_site_for_url(&Url::parse("https://b.example.com:8443/path").unwrap()),
        "https://example.com"
    );
    assert_eq!(
        schemeful_site_for_url(&Url::parse("http://a.example.co.uk/path").unwrap()),
        "http://example.co.uk"
    );
}

#[test]
fn schemeful_site_respects_private_suffix_boundaries() {
    assert_eq!(
        schemeful_site_for_url(&Url::parse("https://foo.github.io/").unwrap()),
        "https://foo.github.io"
    );
    assert_ne!(
        schemeful_site_for_url(&Url::parse("https://foo.github.io/").unwrap()),
        schemeful_site_for_url(&Url::parse("https://bar.github.io/").unwrap())
    );
}

#[test]
fn schemeful_site_falls_back_to_host_for_ip_localhost_and_public_suffix() {
    assert_eq!(
        schemeful_site_for_url(&Url::parse("http://127.0.0.1:8080/path").unwrap()),
        "http://127.0.0.1"
    );
    assert_eq!(
        schemeful_site_for_url(&Url::parse("http://[::1]:8080/path").unwrap()),
        "http://[::1]"
    );
    assert_eq!(
        schemeful_site_for_url(&Url::parse("https://localhost:8443/path").unwrap()),
        "https://localhost"
    );
    assert_eq!(
        schemeful_site_for_url(&Url::parse("https://co.uk/path").unwrap()),
        "https://co.uk"
    );
}

#[test]
fn schemeful_site_handles_file_blob_and_opaque_origins() {
    assert_eq!(
        schemeful_site_for_url(&Url::parse("file:///tmp/page.html").unwrap()),
        "file://"
    );
    assert_eq!(
        schemeful_site_for_url(&Url::parse("file://a.example.com/tmp/page.html").unwrap()),
        "file://example.com"
    );
    assert_eq!(
        schemeful_site_for_url(&Url::parse("blob:https://a.example.com/id").unwrap()),
        "https://example.com"
    );
    assert_eq!(
        schemeful_site_for_url(&Url::parse("data:text/html,hello").unwrap()),
        "null"
    );
}

#[test]
fn host_is_public_suffix_uses_known_psl_rules() {
    assert!(host_is_public_suffix("co.uk"));
    assert!(host_is_public_suffix("github.io"));
    assert!(host_is_public_suffix(".github.io."));
    assert!(!host_is_public_suffix("example.co.uk"));
    assert!(!host_is_public_suffix("foo.github.io"));
    assert!(!host_is_public_suffix("localhost"));
    assert!(!host_is_public_suffix("127.0.0.1"));
}

#[test]
fn public_suffix_list_returns_the_shared_snapshot() {
    let first = public_suffix_list();
    let second = public_suffix_list();

    assert!(Arc::ptr_eq(&first, &second));
}
