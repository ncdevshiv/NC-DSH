use moli_css_parse::{parse_import_rule_view_with_stylo, parse_stylesheet_rule_texts_with_stylo};
use style::stylesheets::CssRuleType;

/// Discovers network requests for the background import-graph fetcher.
///
/// This projection is deliberately not cascade state. Native Stylo
/// `ImportRule` edges remain authoritative for child identity, conditions,
/// parser bases and CSSOM exposure when the fetched responses are installed.
pub(crate) fn stylesheet_top_level_import_urls(
    css_text: &str,
    base_url: &url::Url,
    fail_on_invalid_url: bool,
) -> Result<Vec<url::Url>, ()> {
    stylesheet_top_level_import_state(css_text, base_url, fail_on_invalid_url).map(|(_, urls)| urls)
}

pub(crate) fn stylesheet_top_level_import_state(
    css_text: &str,
    base_url: &url::Url,
    fail_on_invalid_url: bool,
) -> Result<(bool, Vec<url::Url>), ()> {
    let mut has_import_rules = false;
    let mut urls = Vec::new();
    for rule in parse_stylesheet_rule_texts_with_stylo(css_text) {
        if css_rule_text_is_charset(&rule.css_text) {
            continue;
        }
        let Some(import) = parse_import_rule_view_with_stylo(&rule.css_text) else {
            if rule.rule_type == CssRuleType::LayerStatement {
                continue;
            }
            break;
        };
        has_import_rules = true;
        if import.supports_text.as_deref().is_some_and(|condition| {
            !crate::context_bootstrap::css_supports_condition_text(condition)
        }) {
            continue;
        }
        match base_url.join(&import.href) {
            Ok(url) if !urls.iter().any(|existing| existing == &url) => urls.push(url),
            Ok(_) => {}
            Err(_) if !fail_on_invalid_url => {}
            Err(_) => return Err(()),
        }
    }
    Ok((has_import_rules, urls))
}

fn css_rule_text_is_charset(rule_text: &str) -> bool {
    rule_text
        .trim_start()
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("@charset"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_top_level_import_urls_against_the_processing_base() {
        let urls = stylesheet_top_level_import_urls(
            "@charset \"utf-8\"; @import url(shared.css); @import url(shared.css); .target {}",
            &url::Url::parse("https://example.test/processing/").unwrap(),
            false,
        )
        .unwrap();

        assert_eq!(
            urls,
            vec![url::Url::parse("https://example.test/processing/shared.css").unwrap()]
        );
    }

    #[test]
    fn early_layer_statements_do_not_end_the_top_level_import_phase() {
        let urls = stylesheet_top_level_import_urls(
            "@layer reset, theme; @import url(theme.css) layer(theme); .target {}",
            &url::Url::parse("https://example.test/processing/").unwrap(),
            false,
        )
        .unwrap();

        assert_eq!(
            urls,
            vec![url::Url::parse("https://example.test/processing/theme.css").unwrap()]
        );
    }

    #[test]
    fn unsupported_import_conditions_do_not_schedule_requests() {
        let (has_imports, urls) = stylesheet_top_level_import_state(
            "@import url(skipped.css) supports(unknown-property: impossible); .target {}",
            &url::Url::parse("https://example.test/processing/").unwrap(),
            false,
        )
        .unwrap();

        assert!(has_imports);
        assert!(urls.is_empty());
    }

    #[test]
    fn import_discovery_stops_after_the_import_phase() {
        let urls = stylesheet_top_level_import_urls(
            "@import url(first.css); .target {} @import url(too-late.css);",
            &url::Url::parse("https://example.test/processing/").unwrap(),
            false,
        )
        .unwrap();

        assert_eq!(
            urls,
            vec![url::Url::parse("https://example.test/processing/first.css").unwrap()]
        );
    }
}
