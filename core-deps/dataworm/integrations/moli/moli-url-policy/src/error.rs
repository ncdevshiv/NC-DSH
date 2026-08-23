use std::{error::Error, fmt};

use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UrlRequestContext {
    Fetch,
    XmlHttpRequest,
    Navigation,
    HttpNetworkTransport,
    ServiceWorker,
    WebSocket,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UrlPolicyReason {
    UnsupportedScheme,
    UnsupportedAboutUrl,
    LocalFileCapabilityRequired,
    NonHttpNetworkScheme,
    NonWebSocketScheme,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UrlPolicyError {
    context: UrlRequestContext,
    scheme: String,
    reason: UrlPolicyReason,
}

impl UrlPolicyError {
    pub(crate) fn new(url: &Url, context: UrlRequestContext, reason: UrlPolicyReason) -> Self {
        Self {
            context,
            scheme: url.scheme().to_owned(),
            reason,
        }
    }

    #[must_use]
    pub const fn context(&self) -> UrlRequestContext {
        self.context
    }

    #[must_use]
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    #[must_use]
    pub const fn reason(&self) -> UrlPolicyReason {
        self.reason
    }
}

impl fmt::Display for UrlPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.context, self.reason) {
            (
                UrlRequestContext::Fetch | UrlRequestContext::XmlHttpRequest,
                UrlPolicyReason::UnsupportedScheme,
            ) => write!(
                formatter,
                "URL scheme \"{}\" is not supported.",
                self.scheme
            ),
            (UrlRequestContext::Navigation, UrlPolicyReason::UnsupportedAboutUrl) => {
                formatter.write_str("Only about:blank is supported for navigation.")
            }
            (UrlRequestContext::Navigation, UrlPolicyReason::LocalFileCapabilityRequired) => {
                formatter.write_str(
                    "Navigation to a local file URL requires an explicitly granted browser capability.",
                )
            }
            (UrlRequestContext::Navigation, UrlPolicyReason::UnsupportedScheme) => write!(
                formatter,
                "URL scheme \"{}\" is not supported for navigation.",
                self.scheme
            ),
            (UrlRequestContext::HttpNetworkTransport, UrlPolicyReason::NonHttpNetworkScheme) => {
                write!(
                    formatter,
                    "URL scheme \"{}\" is not supported by the HTTP network transport.",
                    self.scheme
                )
            }
            (UrlRequestContext::ServiceWorker, UrlPolicyReason::UnsupportedScheme) => write!(
                formatter,
                "URL scheme \"{}\" is not supported for service workers.",
                self.scheme
            ),
            (UrlRequestContext::WebSocket, UrlPolicyReason::NonWebSocketScheme) => write!(
                formatter,
                "URL scheme \"{}\" is not supported for WebSocket connections.",
                self.scheme
            ),
            _ => write!(
                formatter,
                "URL scheme \"{}\" is not supported in {:?}.",
                self.scheme, self.context
            ),
        }
    }
}

impl Error for UrlPolicyError {}
