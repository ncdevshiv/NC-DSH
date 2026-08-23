//! Bounded concurrent crawler: BFS with depth/domain/include/exclude
//! constraints, deduplication, robots-aware fetch accounting, and per-host
//! rate limiting. No response caching is performed; every queued URL is
//! fetched through [`WebClient`].

use std::collections::{HashSet, VecDeque};

use std::time::Duration;

use ai_errors::{AiError, WebError};

use crate::web_client::WebClient;

/// A crawled page.
#[derive(Debug, Clone)]
pub struct Page {
    pub url: String,
    pub depth: usize,
    pub title: Option<String>,
    pub text: String,
    pub links: Vec<String>,
    pub status: u16,
}

/// The outcome of a crawl run.
#[derive(Debug, Clone, Default)]
pub struct CrawlResult {
    pub pages: Vec<Page>,
    pub visited: usize,
    pub skipped_by_robots: usize,
    pub failed: usize,
}

/// Crawl constraints.
#[derive(Debug, Clone)]
pub struct CrawlConfig {
    pub max_pages: usize,
    pub max_depth: usize,
    /// Only crawl URLs under these prefixes.
    pub include_prefixes: Vec<String>,
    /// Never crawl URLs matching these prefixes.
    pub exclude_prefixes: Vec<String>,
    /// Restrict to these hostnames.
    pub allowed_hosts: Vec<String>,
    /// Delay between requests to the same host (rate limiting).
    pub polite_delay: Duration,
    /// Concurrency (bounded).
    pub concurrency: usize,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            max_pages: 50,
            max_depth: 3,
            include_prefixes: Vec::new(),
            exclude_prefixes: Vec::new(),
            allowed_hosts: Vec::new(),
            polite_delay: Duration::from_millis(100),
            concurrency: 4,
        }
    }
}

/// A bounded BFS crawler over [`WebClient`].
#[derive(Clone)]
pub struct Crawler {
    client: WebClient,
    config: CrawlConfig,
}

impl Crawler {
    pub fn new(client: WebClient, config: CrawlConfig) -> Self {
        Self { client, config }
    }

    fn allowed_url(&self, url: &str) -> bool {
        if !self.config.include_prefixes.is_empty()
            && !self
                .config
                .include_prefixes
                .iter()
                .any(|p| url.starts_with(p.as_str()))
        {
            return false;
        }
        if self
            .config
            .exclude_prefixes
            .iter()
            .any(|p| url.starts_with(p.as_str()))
        {
            return false;
        }
        if !self.config.allowed_hosts.is_empty() {
            let host = url::Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()))
                .unwrap_or_default();
            if !self
                .config
                .allowed_hosts
                .iter()
                .any(|h| host == *h || host.ends_with(&format!(".{h}")))
            {
                return false;
            }
        }
        true
    }

    /// Crawls from `seed_url` until the configured limits are reached.
    pub async fn crawl(&self, seed_url: &str) -> Result<CrawlResult, AiError> {
        let mut result = CrawlResult::default();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut skipped_robots: HashSet<String> = HashSet::new();

        if !self.allowed_url(seed_url) {
            return Err(AiError::Web(WebError::new(
                "crawl",
                format!("seed URL rejected by crawl constraints: {seed_url}"),
            )));
        }
        queue.push_back((seed_url.to_string(), 0));
        visited.insert(seed_url.to_string());

        let limiter = ai_runtime::ConcurrencyLimiter::new();
        if self.config.concurrency > 0 {
            limiter.set_limit("crawl", self.config.concurrency);
        }

        let mut last_request: std::collections::HashMap<String, std::time::Instant> =
            std::collections::HashMap::new();

        while let Some((url, depth)) = queue.pop_front() {
            if result.pages.len() >= self.config.max_pages {
                break;
            }
            if depth > self.config.max_depth {
                continue;
            }

            // Polite rate limiting per host.
            let host = url::Url::parse(&url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()));
            if let Some(host) = &host {
                if let Some(last) = last_request.get(host) {
                    let elapsed = last.elapsed();
                    if elapsed < self.config.polite_delay {
                        tokio::time::sleep(self.config.polite_delay - elapsed).await;
                    }
                }
                last_request.insert(host.clone(), std::time::Instant::now());
            }

            let _permit = limiter.acquire("crawl").await.map_err(|e| {
                AiError::Web(WebError::new("crawl", format!("concurrency error: {e}")))
            })?;

            match self.client.fetch(&url).await {
                Ok(page) => {
                    result.visited += 1;
                    result.pages.push(Page {
                        url: url.clone(),
                        depth,
                        title: page.title,
                        text: page.text,
                        links: page.links.clone(),
                        status: page.status,
                    });
                    // Enqueue new links within constraints and depth.
                    if depth < self.config.max_depth {
                        for link in page.links {
                            if visited.contains(&link)
                                || skipped_robots.contains(&link)
                                || !self.allowed_url(&link)
                            {
                                continue;
                            }
                            // Robots check happens inside fetch; pre-mark to
                            // avoid refetch loops is not possible without
                            // parsing, so rely on the visited set.
                            visited.insert(link.clone());
                            queue.push_back((link, depth + 1));
                        }
                    }
                }
                Err(e) => {
                    if e.to_string().contains("robots") {
                        skipped_robots.insert(url.clone());
                        result.skipped_by_robots += 1;
                    } else {
                        result.failed += 1;
                    }
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web_client::WebClientConfig;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A tiny real async HTTP server for deterministic crawl tests.
    fn start_test_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let handle = tokio::spawn(async move {
            for _ in 0..32 {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 2048];
                let _ = socket.read(&mut buf).await;
                let path = String::from_utf8_lossy(&buf)
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                let body = match path.as_str() {
                    "/" => {
                        "<html><head><title>Home</title></head><body><h1>Home</h1><a href=\"/a\">A</a><a href=\"/b\">B</a></body></html>"
                    }
                    "/a" => {
                        "<html><head><title>Page A</title></head><body><h1>Page A</h1><a href=\"/\">Home</a></body></html>"
                    }
                    "/b" => {
                        "<html><head><title>Page B</title></head><body><h1>Page B</h1></body></html>"
                    }
                    _ => "<html><body>404</body></html>",
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn crawls_bounded_bfs_with_constraints() {
        let (base, server) = start_test_server();
        let port = url::Url::parse(&base).unwrap().port().unwrap();
        let client = WebClient::new(WebClientConfig {
            // Allow the loopback host and ephemeral port used by the test
            // server (the default policy blocks both).
            policy: ai_security::UrlPolicy::new()
                .allow_private_networks()
                .allow_port(port),
            ..Default::default()
        })
        .unwrap();
        let crawler = Crawler::new(
            client,
            CrawlConfig {
                max_pages: 3,
                max_depth: 2,
                polite_delay: Duration::from_millis(1),
                ..Default::default()
            },
        );
        let result = crawler.crawl(&format!("{base}/")).await.unwrap();
        eprintln!(
            "CRAWL DEBUG: pages={} visited={} skipped_robots={} failed={}",
            result.pages.len(),
            result.visited,
            result.skipped_by_robots,
            result.failed
        );
        assert_eq!(result.pages.len(), 3, "home + a + b");
        let titles: Vec<String> = result
            .pages
            .iter()
            .map(|p| p.title.clone().unwrap_or_default())
            .collect();
        assert!(titles.iter().any(|t| t == "Home"), "{titles:?}");
        assert!(titles.iter().any(|t| t == "Page A"), "{titles:?}");
        assert!(titles.iter().any(|t| t == "Page B"), "{titles:?}");
        assert!(result.failed == 0, "no failures: {result:?}");
        server.abort();
    }
}
