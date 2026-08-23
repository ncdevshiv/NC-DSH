use std::{
    collections::HashSet,
    net::IpAddr,
    sync::{Arc, LazyLock},
};

use publicsuffix::{List as PublicSuffixList, Psl};

#[derive(Clone, Debug, Default)]
struct PublicDomainRules {
    rules: HashSet<String>,
    wildcards: HashSet<String>,
    exceptions: HashSet<String>,
}

static PUBLIC_DOMAIN_RULES: LazyLock<PublicDomainRules> = LazyLock::new(|| {
    // Vendored PSL snapshot copied from Servo's local resource list. Keep the
    // site-context model self-contained inside Moli instead of depending
    // on runtime fetches or cargo-registry paths.
    PublicDomainRules::parse(include_str!("data/public_domains.txt"))
});

static PUBLIC_SUFFIX_LIST: LazyLock<Arc<PublicSuffixList>> = LazyLock::new(|| {
    // Servo's vendored snapshot is a raw rules file, while the `publicsuffix`
    // crate expects the official list sections. Wrap the same bytes instead of
    // carrying a second PSL snapshot just for fork-side rejection.
    let wrapped_list = format!(
        "// BEGIN ICANN DOMAINS\n{}\n// BEGIN PRIVATE DOMAINS\n",
        include_str!("data/public_domains.txt")
    );
    Arc::new(
        PublicSuffixList::from_bytes(wrapped_list.as_bytes())
            .expect("vendored public suffix list must parse"),
    )
});

impl PublicDomainRules {
    fn parse(content: &str) -> PublicDomainRules {
        let mut result = PublicDomainRules::default();
        for item in content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
        {
            if let Some(stripped) = item.strip_prefix('!') {
                result.exceptions.insert(stripped.to_owned());
            } else if let Some(stripped) = item.strip_prefix("*.") {
                result.wildcards.insert(stripped.to_owned());
            } else {
                result.rules.insert(item.to_owned());
            }
        }
        result
    }

    fn suffix_pair<'a>(&self, domain: &'a str) -> (&'a str, &'a str) {
        let domain = domain.trim_start_matches('.');
        let mut suffix = domain;
        let mut prev_suffix = domain;

        for (index, _) in domain.match_indices('.') {
            let next_suffix = &domain[index + 1..];
            if self.exceptions.contains(suffix) {
                return (next_suffix, suffix);
            }
            if self.wildcards.contains(next_suffix) || self.rules.contains(suffix) {
                return (suffix, prev_suffix);
            }
            prev_suffix = suffix;
            suffix = next_suffix;
        }

        (suffix, prev_suffix)
    }

    fn registrable_suffix<'a>(&self, domain: &'a str) -> &'a str {
        let (_, registrable) = self.suffix_pair(domain);
        registrable
    }
}

pub(crate) fn is_ip_host(host: &str) -> bool {
    host.parse::<IpAddr>().is_ok()
}

/// Returns the registrable-site key used for cookie site-data grouping.
pub fn site_key_for_host(host: &str) -> Option<String> {
    let host = host.trim().trim_start_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(registrable_site_host(&host).to_owned())
}

/// Returns whether a host is an explicitly known public suffix.
pub fn host_is_public_suffix(host: &str) -> bool {
    let host = host
        .trim()
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() || is_ip_host(&host) {
        return false;
    }
    PUBLIC_SUFFIX_LIST
        .suffix(host.as_bytes())
        .is_some_and(|suffix| suffix.is_known() && suffix == host.as_bytes())
}

/// Returns the registrable domain for `host`, or the host itself when Chromium
/// would not find an eTLD+1, such as IP literals, localhost, and public suffixes.
pub fn registrable_site_host(host: &str) -> &str {
    if is_ip_host(host) {
        host
    } else {
        PUBLIC_DOMAIN_RULES.registrable_suffix(host)
    }
}

/// Compares two hosts using registrable-domain site semantics.
pub fn same_site_hosts(host_a: &str, host_b: &str) -> bool {
    if is_ip_host(host_a) || is_ip_host(host_b) {
        return host_a == host_b;
    }

    registrable_site_host(host_a) == registrable_site_host(host_b)
}

/// Returns the shared vendored public suffix list used by cookie stores.
pub fn public_suffix_list() -> Arc<PublicSuffixList> {
    Arc::clone(&PUBLIC_SUFFIX_LIST)
}
