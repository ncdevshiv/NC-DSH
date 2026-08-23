//! Search providers behind a common trait. The native implementation
//! queries DuckDuckGo's HTML endpoint (no API key required).

use std::time::Duration;

use async_trait::async_trait;
use scraper::{Html, Selector};

use ai_errors::{AiError, WebError};

/// A single search hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// A search provider (web search for agents/research).
#[async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, AiError>;
}

/// Native search via the DuckDuckGo HTML endpoint. Rate-limit friendly and
/// keyless; results are parsed from real HTML.
pub struct DuckDuckGoSearch {
    client: reqwest::Client,
}

impl Default for DuckDuckGoSearch {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (compatible; ai-sdk/0.1)")
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest client builds"),
        }
    }
}

impl DuckDuckGoSearch {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SearchProvider for DuckDuckGoSearch {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, AiError> {
        let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding(query));
        let response = self
            .client
            .get(&url)
            .header("Accept", "text/html")
            .send()
            .await
            .map_err(|e| AiError::Web(WebError::new("search", e.to_string())))?;

        if !response.status().is_success() {
            return Err(AiError::Web(WebError::new(
                "search",
                format!("search engine returned HTTP {}", response.status()),
            )));
        }
        let html = response
            .text()
            .await
            .map_err(|e| AiError::Web(WebError::new("search", e.to_string())))?;

        Ok(parse_duckduckgo_results(&html, limit))
    }
}

fn urlencoding(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Parses DuckDuckGo HTML results (`result__a` links + `result__snippet`).
fn parse_duckduckgo_results(html: &str, limit: usize) -> Vec<SearchResult> {
    let document = Html::parse_document(html);
    let link_selector = Selector::parse("a.result__a").expect("valid selector");
    let snippet_selector = Selector::parse(".result__snippet").expect("valid selector");

    let links: Vec<(String, String)> = document
        .select(&link_selector)
        .filter_map(|element| {
            let title = element.text().collect::<String>().trim().to_string();
            let href = element.value().attr("href")?;
            let url = decode_ddg_redirect(href);
            (!title.is_empty()).then_some((title, url))
        })
        .take(limit)
        .collect();

    let snippets: Vec<String> = document
        .select(&snippet_selector)
        .map(|element| element.text().collect::<String>().trim().to_string())
        .take(limit)
        .collect();

    links
        .into_iter()
        .enumerate()
        .map(|(i, (title, url))| SearchResult {
            title,
            url,
            snippet: snippets.get(i).cloned().unwrap_or_default(),
        })
        .collect()
}

/// DuckDuckGo wraps result URLs in a redirect (`//duckduckgo.com/l/?uddg=...`).
fn decode_ddg_redirect(href: &str) -> String {
    let decoded = percent_decode(href);
    if let Some(param) = decoded.split("uddg=").nth(1) {
        percent_decode(param.split('&').next().unwrap_or(param))
    } else {
        decoded
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len() + 1
            && i + 2 <= bytes.len() - 1 + 1
            && i + 2 < bytes.len() + 1
        {
            if let Some(hex) = hex_pair(bytes, i + 1) {
                out.push(hex);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_pair(bytes: &[u8], start: usize) -> Option<u8> {
    if start + 1 > bytes.len() {
        return None;
    }
    let hi = hex_val(*bytes.get(start)?)?;
    let lo = hex_val(*bytes.get(start + 1)?)?;
    Some(hi * 16 + lo)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encoding_is_standard() {
        assert_eq!(urlencoding("rust ai sdk"), "rust+ai+sdk");
        assert_eq!(urlencoding("a/b?c"), "a%2Fb%3Fc");
    }

    #[test]
    fn percent_decoding_handles_pairs() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("a%2Fb"), "a/b");
    }

    #[test]
    fn parses_real_ddg_html_shape() {
        let html = r##"
            <html><body>
            <div class="result">
                <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage">Example Page</a>
                <div class="result__snippet">A snippet about example.</div>
            </div>
            <a class="result__a" href="https://plain.example.com/x">Plain Link</a>
            <div class="result__snippet">Second snippet.</div>
            </body></html>
        "##;
        let results = parse_duckduckgo_results(html, 5);
        assert_eq!(results.len(), 2, "{results:?}");
        assert_eq!(results[0].title, "Example Page");
        assert_eq!(results[0].url, "https://example.com/page");
        assert_eq!(results[0].snippet, "A snippet about example.");
        assert_eq!(results[1].url, "https://plain.example.com/x");
    }

    #[test]
    fn limit_is_respected() {
        let html = r##"
            <a class="result__a" href="https://a.example">A</a>
            <a class="result__a" href="https://b.example">B</a>
            <a class="result__a" href="https://c.example">C</a>
        "##;
        assert_eq!(parse_duckduckgo_results(html, 2).len(), 2);
    }
}
