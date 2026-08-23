//! Web subsystem (spec §11–§12): real fetching, HTML extraction, robots
//! policy, bounded crawling, search providers, and a self-hosted
//! research layer — all behind a native implementation, no fake backends.

mod crawler;
mod extract;
mod research;
mod robots;
mod search;
mod sitemap;

pub use crawler::{CrawlConfig, CrawlResult, Crawler, Page};
pub use extract::{extract_links, extract_metadata, html_to_text};
pub use research::{
    ExtractRequest, LlmStructuredExtractor, MapRequest, NativeResearchBackend, ResearchBackend,
    ScrapeRequest, SearchRequest, StructuredExtractor,
};
pub use robots::RobotsPolicy;
pub use search::{DuckDuckGoSearch, SearchProvider, SearchResult};
pub use sitemap::{SitemapEntry, SitemapIndexEntry, SitemapParser};
pub use web_client::{FetchedPage, WebClient, WebClientConfig};

mod web_client {
    //! HTTP fetching with SSRF policy, size limits, and redirect handling.

    use std::time::Duration;

    use ai_errors::{AiError, WebError};

    use crate::extract::{extract_links, extract_metadata, html_to_text};
    use crate::robots::RobotsPolicy;

    /// A fetched page with extracted content.
    #[derive(Debug, Clone)]
    pub struct FetchedPage {
        pub url: String,
        pub final_url: String,
        pub status: u16,
        pub content_type: String,
        pub title: Option<String>,
        pub text: String,
        pub links: Vec<String>,
        pub html: String,
    }

    /// Configuration for the web client.
    #[derive(Debug, Clone)]
    pub struct WebClientConfig {
        pub policy: ai_security::UrlPolicy,
        pub user_agent: String,
        pub timeout: Duration,
        pub max_bytes: usize,
    }

    impl Default for WebClientConfig {
        fn default() -> Self {
            Self {
                policy: ai_security::UrlPolicy::new(),
                user_agent: "ai-sdk-web/0.1".to_string(),
                timeout: Duration::from_secs(15),
                max_bytes: 2 * 1024 * 1024,
            }
        }
    }

    /// Fetches and extracts pages.
    #[derive(Clone)]
    pub struct WebClient {
        config: WebClientConfig,
        client: reqwest::Client,
        robots: RobotsPolicy,
    }

    impl std::fmt::Debug for WebClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WebClient")
                .field("config", &self.config)
                .field("robots", &self.robots)
                .finish()
        }
    }

    impl WebClient {
        pub fn new(config: WebClientConfig) -> Result<Self, AiError> {
            let client = reqwest::Client::builder()
                .user_agent(&config.user_agent)
                .timeout(config.timeout)
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .map_err(|e| AiError::Web(WebError::new("client build", e.to_string())))?;
            Ok(Self {
                config,
                client,
                robots: RobotsPolicy::default(),
            })
        }

        pub fn with_robots(mut self, robots: RobotsPolicy) -> Self {
            self.robots = robots;
            self
        }

        /// Fetches a URL, applying the SSRF policy, robots policy, size
        /// limits, and content extraction.
        pub async fn fetch(&self, url: &str) -> Result<FetchedPage, AiError> {
            self.config.policy.require(url)?;
            if !self.robots.allowed(url) {
                return Err(AiError::Web(WebError::new(
                    "fetch",
                    format!("disallowed by robots policy: {url}"),
                )));
            }

            let response = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|e| AiError::Web(WebError::new("fetch", e.to_string())))?;

            let status = response.status().as_u16();
            let final_url = response.url().to_string();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            let bytes = response
                .bytes()
                .await
                .map_err(|e| AiError::Web(WebError::new("fetch body", e.to_string())))?;
            if bytes.len() > self.config.max_bytes {
                return Err(AiError::Web(WebError::new(
                    "fetch",
                    format!("response exceeds {} bytes", self.config.max_bytes),
                )));
            }

            // Decode: assume UTF-8; if the bytes are not valid UTF-8, use
            // encoding_rs with the declared charset or a Windows-1252
            // fallback for legacy HTML.
            let html = decode_html(&bytes, &content_type);

            let text = html_to_text(&html);
            let (title, _) = extract_metadata(&html);
            let links = extract_links(&html, &final_url);

            Ok(FetchedPage {
                url: url.to_string(),
                final_url,
                status,
                content_type,
                title,
                text,
                links,
                html,
            })
        }
    }

    /// Decodes bytes to a String honoring a declared charset (or UTF-8).
    fn decode_html(bytes: &[u8], content_type: &str) -> String {
        let declared = content_type
            .split(';')
            .nth(1)
            .and_then(|p| p.split('=').nth(1))
            .map(|c| c.trim().trim_matches('"').to_lowercase());

        if let Some(charset) = declared {
            if let Some(encoding) = encoding_rs::Encoding::for_label(charset.as_bytes()) {
                let (text, _, _) = encoding.decode(bytes);
                return text.into_owned();
            }
        }
        match std::str::from_utf8(bytes) {
            Ok(text) => text.to_string(),
            Err(_) => {
                let (text, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
                text.into_owned()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn decode_html_for_test(bytes: &[u8], content_type: &str) -> String {
        decode_html(bytes, content_type)
    }
}
