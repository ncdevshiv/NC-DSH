use url::Url;

/// Lightweight site component used by `MoliStorageKey`.
///
/// This stores the serialized schemeful site used for partitioning, usually
/// `scheme://registrable-domain`. Keep it as a distinct type so future work can
/// replace serialization details without changing every storage-key user.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MoliSite {
    serialized: String,
}

impl MoliSite {
    /// Wrap a caller-provided site serialization.
    pub fn new(serialized: String) -> Self {
        Self { serialized }
    }

    /// Compute the current Moli schemeful site for a URL.
    pub fn from_url(url: &Url) -> Self {
        Self::new(site_for_url(url))
    }

    /// Return the serialized site string.
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Consume the site and return its serialized form.
    pub fn into_string(self) -> String {
        self.serialized
    }
}

/// Compute Moli's schemeful registrable site for storage partitioning.
pub fn site_for_url(url: &Url) -> String {
    moli_site::schemeful_site_for_url(url)
}

pub(crate) fn site_for_serialized_origin(origin: &str) -> String {
    Url::parse(origin)
        .map(|url| site_for_url(&url))
        .unwrap_or_else(|_| origin.to_owned())
}

/// Return whether a URL needs an internal opaque-origin nonce.
///
/// The nonce is required whenever the public serialized origin is not unique
/// enough to route storage or messaging state safely.
pub fn url_needs_opaque_nonce(url: &Url) -> bool {
    moli_url::is_opaque_origin(url)
}
