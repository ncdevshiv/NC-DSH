//! Real XML Sitemap and Sitemap Index parser for Web research crawling.
//!
//! Extracts URLs, modification dates, and priority metadata from standard
//! `sitemap.xml` and `sitemapindex.xml` payloads.

use serde::{Deserialize, Serialize};

use ai_errors::{AiError, SerializationError};

/// An entry in a standard `<urlset>` sitemap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SitemapEntry {
    pub loc: String,
    #[serde(default)]
    pub lastmod: Option<String>,
    #[serde(default)]
    pub changefreq: Option<String>,
    #[serde(default)]
    pub priority: Option<f32>,
}

/// An entry in a `<sitemapindex>` payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SitemapIndexEntry {
    pub loc: String,
    #[serde(default)]
    pub lastmod: Option<String>,
}

/// Parser for sitemap XML payloads.
pub struct SitemapParser;

impl SitemapParser {
    /// Parses a `<urlset>` XML string into a vector of [`SitemapEntry`].
    pub fn parse_sitemap(xml: &str) -> Result<Vec<SitemapEntry>, AiError> {
        let mut entries = Vec::new();
        // Regex / string parsing strategy for lightweight, robust XML extraction
        let re_loc = regex::Regex::new(r"(?s)<url>.*?<loc>\s*(.*?)\s*</loc>.*?</url>").unwrap();
        let re_lastmod = regex::Regex::new(r"(?s)<lastmod>\s*(.*?)\s*</lastmod>").unwrap();
        let re_changefreq = regex::Regex::new(r"(?s)<changefreq>\s*(.*?)\s*</changefreq>").unwrap();
        let re_priority = regex::Regex::new(r"(?s)<priority>\s*(.*?)\s*</priority>").unwrap();

        for cap in re_loc.captures_iter(xml) {
            let block = cap.get(0).map_or("", |m| m.as_str());
            let loc = cap.get(1).map_or("", |m| m.as_str()).to_string();

            if loc.is_empty() {
                continue;
            }

            let lastmod = re_lastmod
                .captures(block)
                .map(|c| c.get(1).unwrap().as_str().to_string());
            let changefreq = re_changefreq
                .captures(block)
                .map(|c| c.get(1).unwrap().as_str().to_string());
            let priority = re_priority
                .captures(block)
                .and_then(|c| c.get(1).unwrap().as_str().parse::<f32>().ok());

            entries.push(SitemapEntry {
                loc,
                lastmod,
                changefreq,
                priority,
            });
        }

        if entries.is_empty() && xml.contains("<url>") {
            return Err(AiError::Serialization(SerializationError::new(
                "failed to parse any valid <url> blocks in sitemap",
            )));
        }

        Ok(entries)
    }

    /// Parses a `<sitemapindex>` XML string into a vector of [`SitemapIndexEntry`].
    pub fn parse_index(xml: &str) -> Result<Vec<SitemapIndexEntry>, AiError> {
        let mut entries = Vec::new();
        let re_sitemap =
            regex::Regex::new(r"(?s)<sitemap>.*?<loc>\s*(.*?)\s*</loc>.*?</sitemap>").unwrap();
        let re_lastmod = regex::Regex::new(r"(?s)<lastmod>\s*(.*?)\s*</lastmod>").unwrap();

        for cap in re_sitemap.captures_iter(xml) {
            let block = cap.get(0).map_or("", |m| m.as_str());
            let loc = cap.get(1).map_or("", |m| m.as_str()).to_string();

            if loc.is_empty() {
                continue;
            }

            let lastmod = re_lastmod
                .captures(block)
                .map(|c| c.get(1).unwrap().as_str().to_string());

            entries.push(SitemapIndexEntry { loc, lastmod });
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_sitemap_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
            <url>
                <loc>https://example.com/page1</loc>
                <lastmod>2026-08-01</lastmod>
                <changefreq>daily</changefreq>
                <priority>0.8</priority>
            </url>
            <url>
                <loc>https://example.com/page2</loc>
                <priority>0.5</priority>
            </url>
        </urlset>"#;

        let entries = SitemapParser::parse_sitemap(xml).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].loc, "https://example.com/page1");
        assert_eq!(entries[0].lastmod.as_deref(), Some("2026-08-01"));
        assert_eq!(entries[0].priority, Some(0.8));
        assert_eq!(entries[1].loc, "https://example.com/page2");
        assert_eq!(entries[1].priority, Some(0.5));
    }

    #[test]
    fn parses_sitemap_index_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
            <sitemap>
                <loc>https://example.com/sitemap1.xml</loc>
                <lastmod>2026-08-10</lastmod>
            </sitemap>
        </sitemapindex>"#;

        let entries = SitemapParser::parse_index(xml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].loc, "https://example.com/sitemap1.xml");
    }
}
