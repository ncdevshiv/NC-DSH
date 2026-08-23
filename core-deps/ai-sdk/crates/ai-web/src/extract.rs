//! HTML content extraction: HTML→text, metadata, link discovery.

use std::collections::HashSet;

use scraper::{Html, Selector};

/// Converts HTML to readable text: strips scripts/styles, decodes entities,
/// collapses whitespace, and separates block elements with newlines.
pub fn html_to_text(html: &str) -> String {
    let document = Html::parse_document(html);
    let mut out = String::new();

    for node in document.tree.nodes() {
        use scraper::node::Node;
        match node.value() {
            Node::Text(text) => {
                // Skip text inside script/style/noscript/svg subtrees.
                let mut in_skip = false;
                let mut ancestor = node.parent();
                while let Some(parent) = ancestor {
                    if let Node::Element(el) = parent.value() {
                        if matches!(el.name(), "script" | "style" | "noscript" | "svg") {
                            in_skip = true;
                            break;
                        }
                    }
                    ancestor = parent.parent();
                }
                if !in_skip {
                    out.push_str(text);
                    out.push(' ');
                }
            }
            Node::Element(element) => {
                if matches!(
                    element.name(),
                    "p" | "div"
                        | "br"
                        | "li"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "tr"
                        | "section"
                        | "blockquote"
                ) {
                    out.push('\n');
                }
            }
            _ => {}
        }
    }

    // Collapse runs of whitespace/newlines.
    let mut result = String::with_capacity(out.len());
    let mut prev_ws = false;
    let mut prev_nl = false;
    for ch in out.chars() {
        if ch == '\n' {
            if !prev_nl {
                result.push('\n');
            }
            prev_nl = true;
            prev_ws = false;
        } else if ch.is_whitespace() {
            if !prev_ws && !prev_nl {
                result.push(' ');
            }
            prev_ws = true;
        } else {
            result.push(ch);
            prev_ws = false;
            prev_nl = false;
        }
    }
    result.trim().to_string()
}

/// Extracts the document title and meta description.
pub fn extract_metadata(html: &str) -> (Option<String>, Option<String>) {
    let document = Html::parse_document(html);
    let title_selector = Selector::parse("title").expect("valid selector");
    let title = document
        .select(&title_selector)
        .next()
        .map(|e| e.text().collect::<String>())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let meta_selector = Selector::parse("meta[name='description']").expect("valid selector");
    let description = document
        .select(&meta_selector)
        .next()
        .and_then(|e| e.value().attr("content"))
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty());

    (title, description)
}

/// Discovers absolute links (`href`) in the page, resolved against `base`.
pub fn extract_links(html: &str, base: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").expect("valid selector");

    let mut links = Vec::new();
    let mut seen = HashSet::new();
    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            if let Ok(resolved) = url::Url::parse(base).and_then(|b| b.join(href)) {
                let scheme = resolved.scheme();
                if scheme != "http" && scheme != "https" {
                    continue;
                }
                // Strip fragments; keep query strings.
                let mut normalized = resolved;
                normalized.set_fragment(None);
                let key = normalized.as_str().to_string();
                if seen.insert(key.clone()) {
                    links.push(key);
                }
            }
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_html() {
        let html = r#"
            <html><head><title>Test</title></head>
            <body>
                <script>var x = 1;</script>
                <h1>Hello</h1>
                <p>This is   a   paragraph.</p>
                <ul><li>item one</li><li>item two</li></ul>
                <style>.x{}</style>
            </body></html>
        "#;
        let text = html_to_text(html);
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("This is a paragraph."), "{text}");
        assert!(text.contains("item one"), "{text}");
        assert!(text.contains("item two"), "{text}");
        assert!(!text.contains("var x"), "script content stripped: {text}");
    }

    #[test]
    fn extracts_metadata() {
        let html = r#"<html><head><title>My Page</title><meta name="description" content="A page about things"></head><body></body></html>"#;
        let (title, description) = extract_metadata(html);
        assert_eq!(title.as_deref(), Some("My Page"));
        assert_eq!(description.as_deref(), Some("A page about things"));
    }

    #[test]
    fn discovers_and_normalizes_links() {
        let html = r#"
            <a href="/about">About</a>
            <a href="https://example.com/contact#section">Contact</a>
            <a href="mailto:x@y.z">Mail</a>
            <a href="/about">Duplicate</a>
        "#;
        let links = extract_links(html, "https://example.com/");
        assert_eq!(links.len(), 2, "{links:?}");
        assert!(links.contains(&"https://example.com/about".to_string()));
        assert!(links.contains(&"https://example.com/contact".to_string()));
        assert!(!links.iter().any(|l| l.contains("mailto")));
    }

    #[test]
    fn decodes_legacy_encoding() {
        // "café" in Windows-1252 bytes.
        let bytes = [b'c', b'a', b'f', 0xE9];
        let text =
            crate::web_client::decode_html_for_test(&bytes, "text/html; charset=windows-1252");
        assert_eq!(text, "café");
    }
}
