use url::Url;

/// Browser-relevant URL scheme classes.
///
/// These classes deliberately do not collapse all "local" schemes together:
/// Chromium routes data, blob, about, and file URLs through different loaders
/// and grants them different capabilities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserUrlScheme {
    Http,
    Https,
    Data,
    Blob,
    About,
    File,
    WebSocket,
    SecureWebSocket,
    Other,
}

impl BrowserUrlScheme {
    #[must_use]
    pub fn from_url(url: &Url) -> Self {
        match url.scheme() {
            "http" => Self::Http,
            "https" => Self::Https,
            "data" => Self::Data,
            "blob" => Self::Blob,
            "about" => Self::About,
            "file" => Self::File,
            "ws" => Self::WebSocket,
            "wss" => Self::SecureWebSocket,
            _ => Self::Other,
        }
    }

    /// Matches Chromium's CORS-enabled scheme registry defaults.
    #[must_use]
    pub const fn is_cors_enabled(self) -> bool {
        matches!(self, Self::Http | Self::Https | Self::Data)
    }

    /// Matches Chromium's Fetch API scheme registry.
    ///
    /// Data and blob URLs are still fetchable, but through dedicated local
    /// fetch routes instead of the registered HTTP Fetch API route.
    #[must_use]
    pub const fn supports_fetch_api(self) -> bool {
        matches!(self, Self::Http | Self::Https)
    }

    #[must_use]
    pub const fn supports_service_worker(self) -> bool {
        matches!(self, Self::Http | Self::Https)
    }

    #[must_use]
    pub const fn is_local(self) -> bool {
        matches!(self, Self::File)
    }

    #[must_use]
    pub const fn uses_http_network_transport(self) -> bool {
        matches!(self, Self::Http | Self::Https)
    }
}
