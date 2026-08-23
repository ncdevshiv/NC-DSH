use super::{QueryEngine, assert_query_ids, find_by_id, parse_document};
use crate::dom::native::DomHost;

#[test]
fn matches_attribute_operators() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="a" data-val="hello-world" data-tag="en-US" data-words="foo bar baz" data-empty=""></div>
        <div id="b" data-val="hello" data-tag="en" data-words="alpha"></div>
        <div id="c" data-val="world" data-tag="fr-CA" data-words="baz"></div>
        <div id="d"></div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            // existence
            ("[data-val]", vec!["a", "b", "c"]),
            ("[data-empty]", vec!["a"]),
            // exact match
            (r#"[data-val="hello"]"#, vec!["b"]),
            (r#"[data-val="hello-world"]"#, vec!["a"]),
            // prefix
            (r#"[data-val^="hello"]"#, vec!["a", "b"]),
            (r#"[data-val^="world"]"#, vec!["c"]),
            // suffix
            (r#"[data-val$="world"]"#, vec!["a", "c"]),
            (r#"[data-val$="hello"]"#, vec!["b"]),
            // substring
            (r#"[data-val*="llo"]"#, vec!["a", "b"]),
            (r#"[data-val*="orl"]"#, vec!["a", "c"]),
            // word list
            (r#"[data-words~="foo"]"#, vec!["a"]),
            (r#"[data-words~="baz"]"#, vec!["a", "c"]),
            (r#"[data-words~="alpha"]"#, vec!["b"]),
            // hyphen / lang
            (r#"[data-tag|="en"]"#, vec!["a", "b"]),
            (r#"[data-tag|="fr"]"#, vec!["c"]),
            // case-insensitive flag
            (r#"[data-val="HELLO" i]"#, vec!["b"]),
            (r#"[data-val^="HELLO" i]"#, vec!["a", "b"]),
        ],
    );
}

#[test]
fn matches_multiple_attribute_conditions() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <input id="x" type="text" required />
        <input id="y" type="text" />
        <input id="z" type="checkbox" required />
        </body></html>"#,
    );
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            (r#"[type="text"][required]"#, vec!["x"]),
            (r#"[type="checkbox"][required]"#, vec!["z"]),
            (r#"[type="text"]:not([required])"#, vec!["y"]),
        ],
    );
}

#[test]
fn matches_attribute_values_with_commas_inside_quotes() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <link
          id="preload-link"
          rel="preload"
          as="image"
          imagesrcset="url1.png 1x, url2.png 2x">
        </body></html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;
    let expected = Some(find_by_id(&document, "preload-link"));

    let selectors = [
        r#"link[rel="preload"][as="image"][imagesrcset="url1.png 1x, url2.png 2x"]"#,
        r#"link[imagesrcset='url1.png 1x, url2.png 2x']"#,
    ];

    for selector in selectors {
        assert_eq!(
            engine
                .query_selector(&document, selector)
                .unwrap_or_else(|e| panic!("tree selector {selector:?} failed: {e:?}")),
            expected,
            "tree selector {selector:?}"
        );
        assert_eq!(
            engine
                .query_selector_host(&host, selector)
                .unwrap_or_else(|e| panic!("host selector {selector:?} failed: {e:?}")),
            expected,
            "host selector {selector:?}"
        );
    }
}

#[test]
fn query_selector_case_insensitive_attribute_substring_matches_servo_wpt() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <input id="test-input" name="User">
        <input id="other-input" name="Admin">
        </body></html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;
    let expected = Some(find_by_id(&document, "test-input"));

    assert_eq!(
        engine
            .query_selector(&document, r#"input[name*=user i]"#)
            .unwrap_or_else(|e| panic!("tree query_selector failed: {e:?}")),
        expected
    );
    assert_eq!(
        engine
            .query_selector_host(&host, r#"input[name*=user i]"#)
            .unwrap_or_else(|e| panic!("host query_selector failed: {e:?}")),
        expected
    );
    assert_eq!(
        engine
            .query_selector_all(&document, r#"input[name*=user i]"#)
            .unwrap_or_else(|e| panic!("tree query_selector_all failed: {e:?}")),
        vec![find_by_id(&document, "test-input")]
    );
    assert_eq!(
        engine
            .query_selector_all_host(&host, r#"input[name*=user i]"#)
            .unwrap_or_else(|e| panic!("host query_selector_all failed: {e:?}")),
        vec![find_by_id(&document, "test-input")]
    );
}

#[test]
fn query_selector_mixed_case_attribute_names_and_flags_match_html_wpt_subset() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="html1" testAttr="alpha" dataIndex="0"></div>
        <div id="html2" DataType="primary"></div>
        <div id="html3" datatype="secondary"></div>
        </body></html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;

    let cases: &[(&str, Vec<&str>)] = &[
        (r#"[testAttr]"#, vec!["html1"]),
        (r#"[testattr]"#, vec!["html1"]),
        (r#"[dataIndex="0"]"#, vec!["html1"]),
        (r#"[dataindex="0"]"#, vec!["html1"]),
        (r#"[testAttr="alpha" s]"#, vec!["html1"]),
        (r#"[testAttr="ALPHA" i]"#, vec!["html1"]),
        (r#"[DataType]"#, vec!["html2", "html3"]),
        (r#"[datatype]"#, vec!["html2", "html3"]),
        (r#"[datatype="primary" i]"#, vec!["html2"]),
        (r#"[datatype="SECONDARY" i]"#, vec!["html3"]),
        (r#"[datatype="SECONDARY" s]"#, vec![]),
    ];

    for &(selector, ref expected_ids) in cases {
        let got_tree: Vec<&str> = engine
            .query_selector_all(&document, selector)
            .unwrap_or_else(|e| panic!("tree selector {selector:?} failed: {e:?}"))
            .iter()
            .map(|nid| {
                document
                    .node(*nid)
                    .and_then(|n| n.kind().as_element())
                    .and_then(|el| el.attribute("id"))
                    .unwrap_or("")
            })
            .collect();
        assert_eq!(got_tree, *expected_ids, "tree selector {selector:?}");

        let got_host: Vec<&str> = engine
            .query_selector_all_host(&host, selector)
            .unwrap_or_else(|e| panic!("host selector {selector:?} failed: {e:?}"))
            .iter()
            .map(|nid| {
                document
                    .node(*nid)
                    .and_then(|n| n.kind().as_element())
                    .and_then(|el| el.attribute("id"))
                    .unwrap_or("")
            })
            .collect();
        assert_eq!(got_host, *expected_ids, "host selector {selector:?}");
    }
}

#[test]
fn matches_uppercase_attribute_names_in_html() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="case-target" data-test="value1" data-extra="two" data-empty="" class="foo  bar" lang="en-US">Target</div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    // Case-insensitive attribute name (DATA-TEST should match data-test in HTML)
    let result = engine
        .query_selector(&document, r#"[DATA-TEST="value1"]"#)
        .expect("query should succeed");
    assert_eq!(
        result,
        Some(find_by_id(&document, "case-target")),
        "uppercase attr name lookup"
    );

    // Whitespace around operator
    let result = engine
        .query_selector(&document, r#"[ data-test = "value1" ]"#)
        .expect("query should succeed");
    assert_eq!(
        result,
        Some(find_by_id(&document, "case-target")),
        "whitespace around operator"
    );
}

#[test]
fn query_selector_matches_attribute_values_with_spaces_and_dashes() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <a id="testme" title="test with - dash and space">Test One</a>
        <a id="other" title="different title">Test Two</a>
        </body></html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;
    let expected = Some(find_by_id(&document, "testme"));
    let selector = r#"a[title='test with - dash and space']"#;

    assert_eq!(
        engine
            .query_selector(&document, selector)
            .unwrap_or_else(|e| panic!("tree query_selector failed: {e:?}")),
        expected
    );
    assert_eq!(
        engine
            .query_selector_host(&host, selector)
            .unwrap_or_else(|e| panic!("host query_selector failed: {e:?}")),
        expected
    );
    assert_eq!(
        engine
            .query_selector_all(&document, selector)
            .unwrap_or_else(|e| panic!("tree query_selector_all failed: {e:?}")),
        vec![find_by_id(&document, "testme")]
    );
    assert_eq!(
        engine
            .query_selector_all_host(&host, selector)
            .unwrap_or_else(|e| panic!("host query_selector_all failed: {e:?}")),
        vec![find_by_id(&document, "testme")]
    );
}

#[test]
fn attribute_selector_edge_cases() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="target" data-flag="" DATA-UPPER="yes"></div>
        </body></html>"#,
    );
    let engine = QueryEngine;
    let target = find_by_id(&document, "target");

    // Empty attribute value – exists
    assert!(
        engine
            .matches(&document, target, "[data-flag]")
            .expect("ok")
    );
    // Empty attribute value – exact empty string
    assert!(
        engine
            .matches(&document, target, r#"[data-flag=""]"#)
            .expect("ok")
    );
    // Empty attribute value does not match non-empty
    assert!(
        !engine
            .matches(&document, target, r#"[data-flag="x"]"#)
            .expect("ok")
    );
    // HTML case-insensitive attribute names
    assert!(
        engine
            .matches(&document, target, "[data-upper]")
            .expect("ok"),
        "upper-cased attribute in HTML should be accessible by lowercase name"
    );
}
