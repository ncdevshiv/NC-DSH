//! robots.txt policy: a real parser for robots exclusion rules.

/// A parsed robots.txt policy with `User-agent: *` rules.
///
/// Supports `Disallow` and `Allow` rules (longest-match wins, per the
/// original spec; wildcards are not required by the base spec). Defaults to
/// *allowing* everything when no rules apply.
#[derive(Debug, Clone, Default)]
pub struct RobotsPolicy {
    /// (allow, path) pairs; allow=true for `Allow` rules.
    rules: Vec<(bool, String)>,
    /// Whether the agent name was matched by any user-agent line.
    matched: bool,
}

impl RobotsPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses robots.txt content for our user agent. Lines outside the
    /// relevant `User-agent` group are ignored.
    pub fn parse(content: &str, user_agent: &str) -> Self {
        let mut policy = Self::default();
        let mut in_group = false;
        let agent = user_agent.to_lowercase();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((f, v)) => (f.trim().to_lowercase(), v.trim()),
                None => continue,
            };

            match field.as_str() {
                "user-agent" => {
                    let name = value.to_lowercase();
                    // Match the given agent or the catch-all `*`.
                    in_group = name == agent || name == "*" || agent.starts_with(&name);
                    if in_group {
                        policy.matched = true;
                    }
                }
                "disallow" if in_group => {
                    if value.is_empty() {
                        // Empty Disallow = allow everything.
                        continue;
                    }
                    policy.rules.push((false, value.to_string()));
                }
                "allow" if in_group && !value.is_empty() => {
                    policy.rules.push((true, value.to_string()));
                }
                _ => {}
            }
        }
        policy
    }

    /// Whether `url` may be fetched under this policy.
    pub fn allowed(&self, url: &str) -> bool {
        if !self.matched || self.rules.is_empty() {
            return true;
        }
        let path = match url::Url::parse(url) {
            Ok(u) => {
                let mut path = u.path().to_string();
                if let Some(query) = u.query() {
                    path.push('?');
                    path.push_str(query);
                }
                path
            }
            Err(_) => url.to_string(),
        };

        // Longest matching rule decides; Allow beats Disallow on ties.
        let mut best: Option<(usize, bool)> = None;
        for (is_allow, rule) in &self.rules {
            if path.starts_with(rule.as_str()) {
                let candidate = (rule.len(), *is_allow);
                match &best {
                    Some((len, _)) if *len >= rule.len() => {}
                    _ => best = Some(candidate),
                }
            }
        }
        best.map(|(_, is_allow)| is_allow).unwrap_or(true)
    }

    /// Fetches and parses robots.txt for a host using `client` (real
    /// network). Falls back to an allow-all policy on fetch errors.
    pub async fn fetch_for(client: &reqwest::Client, url: &str, user_agent: &str) -> Self {
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(_) => return Self::default(),
        };
        let robots_url = format!(
            "{}://{}/robots.txt",
            parsed.scheme(),
            parsed.host_str().unwrap_or("")
        );
        match client
            .get(&robots_url)
            .header("User-Agent", user_agent)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                let body = response.text().await.unwrap_or_default();
                Self::parse(&body, user_agent)
            }
            _ => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disallow_blocks_path() {
        let policy = RobotsPolicy::parse(
            "User-agent: *\nDisallow: /private/\nDisallow: /tmp\n",
            "ai-sdk-web/0.1",
        );
        assert!(!policy.allowed("https://example.com/private/data.html"));
        assert!(!policy.allowed("https://example.com/tmp/file"));
        assert!(policy.allowed("https://example.com/public/page"));
    }

    #[test]
    fn allow_overrides_disallow_on_longest_match() {
        let policy = RobotsPolicy::parse(
            "User-agent: *\nDisallow: /private/\nAllow: /private/open.html\n",
            "bot",
        );
        assert!(policy.allowed("https://example.com/private/open.html"));
        assert!(!policy.allowed("https://example.com/private/closed.html"));
    }

    #[test]
    fn unmatched_agent_allows_everything() {
        let policy = RobotsPolicy::parse("User-agent: other-bot\nDisallow: /\n", "my-bot");
        assert!(policy.allowed("https://example.com/anything"));
    }

    #[test]
    fn no_rules_allows_everything() {
        let policy = RobotsPolicy::parse("User-agent: *\n", "bot");
        assert!(policy.allowed("https://example.com/x"));
    }

    #[test]
    fn empty_disallow_allows() {
        let policy = RobotsPolicy::parse("User-agent: *\nDisallow:\n", "bot");
        assert!(policy.allowed("https://example.com/"));
    }
}
