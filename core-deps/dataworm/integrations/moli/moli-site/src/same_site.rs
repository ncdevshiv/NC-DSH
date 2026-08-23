use url::Url;

use crate::site_key_for_host;

#[derive(Clone, Debug, Eq, PartialEq)]
struct SameSiteComparableUrl {
    scheme: String,
    site: String,
}

/// Compares two URLs using browser SameSite site rules.
pub fn same_site_urls(url_a: &Url, url_b: &Url, schemeful: bool) -> bool {
    let Some(origin_a) = same_site_comparable_url(url_a) else {
        return false;
    };
    let Some(origin_b) = same_site_comparable_url(url_b) else {
        return false;
    };

    origin_a.site == origin_b.site && (!schemeful || origin_a.scheme == origin_b.scheme)
}

fn normalized_same_site_scheme(scheme: &str) -> &str {
    match scheme {
        "ws" => "http",
        "wss" => "https",
        other => other,
    }
}

fn is_same_site_comparable_scheme(scheme: &str) -> bool {
    matches!(scheme, "http" | "https" | "ws" | "wss" | "ftp")
}

fn same_site_comparable_url(url: &Url) -> Option<SameSiteComparableUrl> {
    match url.scheme() {
        "blob" => Url::parse(url.path())
            .ok()
            .and_then(|nested| same_site_comparable_url(&nested)),
        "file" => Some(SameSiteComparableUrl {
            scheme: "file".to_owned(),
            site: url.host_str().unwrap_or_default().to_ascii_lowercase(),
        }),
        _ => {
            if !is_same_site_comparable_scheme(url.scheme()) {
                return None;
            }
            let host = url.host_str()?;
            Some(SameSiteComparableUrl {
                scheme: normalized_same_site_scheme(url.scheme()).to_owned(),
                site: site_key_for_host(host)?,
            })
        }
    }
}
