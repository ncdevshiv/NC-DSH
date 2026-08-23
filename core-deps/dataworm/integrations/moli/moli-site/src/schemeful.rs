use std::net::Ipv6Addr;

use url::{Host, Url};

use crate::registrable_site_host;

/// Computes the serialized schemeful site used for storage partitioning.
///
/// This follows Chromium `net::SchemefulSite`'s obtain-a-site shape for the
/// browser-visible schemes Moli supports: opaque origins serialize as
/// `"null"`, standard network-host schemes use scheme plus registrable domain
/// when available, and otherwise fall back to scheme plus host with port
/// discarded.
pub fn schemeful_site_for_url(url: &Url) -> String {
    match url.scheme() {
        "blob" => {
            return Url::parse(url.path())
                .map(|nested| schemeful_site_for_url(&nested))
                .unwrap_or_else(|_| "null".to_owned());
        }
        "file" => return schemeful_file_site_for_url(url),
        _ => {}
    }
    let Some(origin_url) = moli_url::tuple_origin_url(url) else {
        return "null".to_owned();
    };
    let scheme = origin_url.scheme();
    let Some(host) = origin_url.host() else {
        return if scheme == "file" {
            "file://".to_owned()
        } else {
            moli_url::origin_ascii_serialization(&origin_url)
        };
    };
    let host = if is_standard_scheme_with_network_host(scheme) {
        registrable_domain_or_host(host)
    } else {
        serialize_host(host)
    };
    format!("{scheme}://{host}")
}

fn schemeful_file_site_for_url(url: &Url) -> String {
    let Some(host) = url.host() else {
        return "file://".to_owned();
    };
    format!("file://{}", registrable_domain_or_host(host))
}

fn is_standard_scheme_with_network_host(scheme: &str) -> bool {
    matches!(scheme, "http" | "https" | "ws" | "wss" | "ftp" | "file")
}

fn registrable_domain_or_host(host: Host<&str>) -> String {
    match host {
        Host::Domain(domain) => registrable_site_host(domain).to_owned(),
        Host::Ipv4(ip) => ip.to_string(),
        Host::Ipv6(ip) => serialize_ipv6(ip),
    }
}

fn serialize_host(host: Host<&str>) -> String {
    match host {
        Host::Domain(domain) => domain.to_owned(),
        Host::Ipv4(ip) => ip.to_string(),
        Host::Ipv6(ip) => serialize_ipv6(ip),
    }
}

fn serialize_ipv6(ip: Ipv6Addr) -> String {
    format!("[{ip}]")
}
