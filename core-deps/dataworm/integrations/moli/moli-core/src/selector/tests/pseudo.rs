use super::{QueryEngine, assert_query_ids, find_by_id, parse_document};
use crate::dom::native::DomHost;

#[test]
fn matches_structural_and_attribute_pseudos() {
    let document = parse_document(
        r#"
        <!doctype html>
        <html>
          <body>
            <ul id="list">
              <li data-order="1">One</li>
              <li data-order="2">Two</li>
              <li data-order="3" class="done">Three</li>
              <li data-order="4">Four</li>
            </ul>
            <div id="solo"><span id="only">Only</span></div>
          </body>
        </html>
        "#,
    );
    let engine = QueryEngine;

    assert_eq!(
        engine
            .query_selector(&document, "#list li:nth-child(2)")
            .expect("query should succeed"),
        Some(
            engine
                .query_selector(&document, r#"li[data-order="2"]"#)
                .expect("query should succeed")
                .expect("expected second element")
        )
    );
    assert_eq!(
        engine
            .query_selector(&document, "#list li:nth-last-child(2)")
            .expect("query should succeed"),
        Some(
            engine
                .query_selector(&document, r#"li[data-order="3"]"#)
                .expect("query should succeed")
                .expect("expected third element")
        )
    );
    assert_eq!(
        engine
            .query_selector(&document, "#solo > :only-child")
            .expect("query should succeed"),
        Some(find_by_id(&document, "only"))
    );
    assert_eq!(
        engine
            .query_selector_all(&document, r#"li:not(.done)[data-order^="1"], li.done"#)
            .expect("query should succeed")
            .len(),
        2
    );
}

#[test]
fn matches_static_validity_pseudos_for_parsed_html() {
    let document = parse_document(
        r#"
        <!doctype html>
        <html>
          <body>
            <form id="form">
              <input id="required-empty" required />
              <input id="required-filled" required value="ok" />
              <input id="checked-box" type="checkbox" required checked />
              <input id="missing-box" type="checkbox" required />
              <select id="required-select" required>
                <option value="">Choose</option>
                <option value="x">X</option>
              </select>
            </form>
          </body>
        </html>
        "#,
    );
    let engine = QueryEngine;

    assert!(
        engine
            .matches(
                &document,
                find_by_id(&document, "required-empty"),
                ":invalid"
            )
            .expect("match should succeed")
    );
    assert!(
        engine
            .matches(
                &document,
                find_by_id(&document, "required-filled"),
                ":valid"
            )
            .expect("match should succeed")
    );
    assert!(
        engine
            .matches(&document, find_by_id(&document, "checked-box"), ":checked")
            .expect("match should succeed")
    );
    assert!(
        engine
            .matches(&document, find_by_id(&document, "form"), ":invalid")
            .expect("match should succeed")
    );
    assert!(
        engine
            .matches(&document, find_by_id(&document, "missing-box"), ":invalid")
            .expect("match should succeed")
    );
    assert!(
        engine
            .matches(
                &document,
                find_by_id(&document, "required-select"),
                ":invalid"
            )
            .expect("match should succeed")
    );
}

#[test]
fn matches_first_last_only_pseudos() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <ul id="list">
            <li id="li1">one</li>
            <li id="li2">two</li>
            <li id="li3">three</li>
        </ul>
        <div id="mixed">
            <span id="sp1">a</span>
            <p id="pp1">b</p>
            <span id="sp2">c</span>
        </div>
        <div id="solo"><em id="em1">only</em></div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            ("#list li:first-child", vec!["li1"]),
            ("#list li:last-child", vec!["li3"]),
            ("#list li:only-child", vec![]), // 3 children, none is only
            ("#solo em:only-child", vec!["em1"]),
            // first-of-type / last-of-type across mixed content
            ("#mixed span:first-of-type", vec!["sp1"]),
            ("#mixed span:last-of-type", vec!["sp2"]),
            ("#mixed p:only-of-type", vec!["pp1"]),
            ("#mixed span:only-of-type", vec![]), // two spans
        ],
    );
}

#[test]
fn matches_nth_child_variants() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <ol id="ol">
            <li id="n1">1</li>
            <li id="n2">2</li>
            <li id="n3">3</li>
            <li id="n4">4</li>
            <li id="n5">5</li>
            <li id="n6">6</li>
        </ol>
        <div id="types">
            <span id="s1">a</span>
            <p id="p1">b</p>
            <span id="s2">c</span>
            <p id="p2">d</p>
            <span id="s3">e</span>
        </div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            ("#ol li:nth-child(1)", vec!["n1"]),
            ("#ol li:nth-child(3)", vec!["n3"]),
            ("#ol li:nth-child(odd)", vec!["n1", "n3", "n5"]),
            ("#ol li:nth-child(even)", vec!["n2", "n4", "n6"]),
            ("#ol li:nth-child(2n+1)", vec!["n1", "n3", "n5"]),
            ("#ol li:nth-child(3n)", vec!["n3", "n6"]),
            ("#ol li:nth-last-child(1)", vec!["n6"]),
            ("#ol li:nth-last-child(2)", vec!["n5"]),
            // :nth-of-type -- counts only li siblings of same type
            ("#types span:nth-of-type(2)", vec!["s2"]),
            ("#types p:nth-of-type(1)", vec!["p1"]),
            ("#types span:nth-last-of-type(1)", vec!["s3"]),
        ],
    );
}

#[test]
fn matches_empty_and_root() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="empty-el"></div>
        <div id="text-el">hello</div>
        <div id="child-el"><span></span></div>
        <div id="space-el"> </div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    // :empty matches no children (no text, no elements)
    let empty_ids: Vec<_> = engine
        .query_selector_all(&document, ":empty")
        .expect("should succeed")
        .iter()
        .filter_map(|id| {
            document
                .node(*id)
                .and_then(|n| n.kind().as_element())
                .and_then(|el| el.attribute("id"))
        })
        .collect();

    assert!(
        empty_ids.contains(&"empty-el"),
        "empty-el should be :empty, got: {empty_ids:?}"
    );
    assert!(
        !empty_ids.contains(&"text-el"),
        "text-el should NOT be :empty"
    );
    assert!(
        !empty_ids.contains(&"child-el"),
        "child-el should NOT be :empty"
    );

    // :root matches the html element
    let root = engine
        .query_selector(&document, ":root")
        .expect("should succeed")
        .expect("should find root");
    let root_name = document
        .node(root)
        .and_then(|n| n.kind().as_element())
        .map(|el| el.local_name());
    assert_eq!(root_name, Some("html"), ":root should be <html>");
}

#[test]
fn matches_functional_pseudos_not_is_where_has() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="d1" class="active">
            <span id="s1">inner</span>
        </div>
        <div id="d2">
            <p id="p1" class="active">para</p>
        </div>
        <p id="p2">bare</p>
        <section id="sec"></section>
        </body></html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            // :not() – simple
            ("div:not(.active)", vec!["d2"]),
            ("p:not(.active)", vec!["p2"]),
            // :not() – compound (restricted to body so html/head/body are not included)
            ("body :not(div):not(section)", vec!["s1", "p1", "p2"]),
            // :is() – selector list
            (":is(div, section)", vec!["d1", "d2", "sec"]),
            (":is(.active)", vec!["d1", "p1"]),
            // :where() – same matching as :is() but 0 specificity
            (":where(div, section)", vec!["d1", "d2", "sec"]),
            // :has() – has descendant
            ("div:has(span)", vec!["d1"]),
            ("div:has(p)", vec!["d2"]),
            ("div:has(> span)", vec!["d1"]), // direct child
            ("div:has(> p)", vec!["d2"]),
            // :has() no match
            ("section:has(span)", vec![]),
        ],
    );

    for (selector, expected_ids) in [
        ("div:not(.active)", vec!["d2"]),
        (":is(div, section)", vec!["d1", "d2", "sec"]),
        (":where(div, section)", vec!["d1", "d2", "sec"]),
        ("div:has(span)", vec!["d1"]),
        ("div:has(> p)", vec!["d2"]),
        ("section:has(span)", vec![]),
    ] {
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
        assert_eq!(got_host, expected_ids, "host selector {selector:?}");
    }
}

#[test]
fn matches_not_with_complex_arg() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="a" class="foo" data-x="1"></div>
        <div id="b" class="foo"></div>
        <div id="c" data-x="1"></div>
        <div id="d"></div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    // :not with attribute selector
    assert_query_ids(
        &engine,
        &document,
        &[
            ("div:not([data-x])", vec!["b", "d"]),
            ("div:not(.foo)", vec!["c", "d"]),
            ("div:not(.foo):not([data-x])", vec!["d"]),
        ],
    );
}

#[test]
fn matches_link_pseudo_classes() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <a id="a-href" href="https://example.com">Link</a>
        <a id="a-no-href">Not a link</a>
        <area id="area-href" href="page.html" />
        <link id="link-href" href="style.css" rel="stylesheet" />
        <span id="span-href" href="fake">Span</span>
        </body></html>"#,
    );
    let engine = QueryEngine;

    for pseudo in [":link", ":any-link"] {
        let results = engine
            .query_selector_all(&document, pseudo)
            .unwrap_or_else(|e| panic!("{pseudo} query failed: {e:?}"));
        let ids: Vec<_> = results
            .iter()
            .filter_map(|id| {
                document
                    .node(*id)
                    .and_then(|n| n.kind().as_element())
                    .and_then(|el| el.attribute("id"))
            })
            .collect();
        assert!(ids.contains(&"a-href"), "{pseudo}: a[href] should match");
        assert!(
            !ids.contains(&"a-no-href"),
            "{pseudo}: a without href should NOT match"
        );
        assert!(
            ids.contains(&"area-href"),
            "{pseudo}: area[href] should match"
        );
        assert!(
            !ids.contains(&"link-href"),
            "{pseudo}: link[href] should NOT match"
        );
        // span with href attribute is NOT a link element type
        assert!(
            !ids.contains(&"span-href"),
            "{pseudo}: span should NOT match"
        );
    }

    // :visited always returns false (headless – no browsing history)
    let visited = engine
        .query_selector_all(&document, ":visited")
        .expect(":visited should parse");
    assert!(
        visited.is_empty(),
        ":visited should match nothing in headless mode"
    );
}

#[test]
fn matches_disabled_enabled() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <input id="inp-dis" type="text" disabled />
        <input id="inp-en" type="text" />
        <button id="btn-dis" disabled>dis</button>
        <button id="btn-en">en</button>
        <select id="sel-dis" disabled><option>x</option></select>
        <select id="sel-en"><option>x</option></select>
        <textarea id="ta-dis" disabled></textarea>
        <textarea id="ta-en"></textarea>
        <div id="div-dis" disabled></div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    for id in ["inp-dis", "btn-dis", "sel-dis", "ta-dis"] {
        assert!(
            engine
                .matches(&document, find_by_id(&document, id), ":disabled")
                .expect("should succeed"),
            "{id} should match :disabled"
        );
        assert!(
            !engine
                .matches(&document, find_by_id(&document, id), ":enabled")
                .expect("should succeed"),
            "{id} should NOT match :enabled"
        );
    }
    for id in ["inp-en", "btn-en", "sel-en", "ta-en"] {
        assert!(
            !engine
                .matches(&document, find_by_id(&document, id), ":disabled")
                .expect("should succeed"),
            "{id} should NOT match :disabled"
        );
        assert!(
            engine
                .matches(&document, find_by_id(&document, id), ":enabled")
                .expect("should succeed"),
            "{id} should match :enabled"
        );
    }
    // Non-form element with disabled attr: matches neither pseudo-class.
    assert!(
        !engine
            .matches(&document, find_by_id(&document, "div-dis"), ":disabled")
            .expect("should succeed"),
        "div[disabled] should NOT match :disabled"
    );
    assert!(
        !engine
            .matches(&document, find_by_id(&document, "div-dis"), ":enabled")
            .expect("should succeed"),
        "div[disabled] should NOT match :enabled (not a form element)"
    );
}

#[test]
fn matches_required_optional_readonly_readwrite_placeholder() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <input id="req" type="text" required />
        <input id="opt" type="text" />
        <input id="ro"  type="text" readonly />
        <input id="rw"  type="text" />
        <input id="ph"  type="text" placeholder="hint" />
        <textarea id="ta-req" required></textarea>
        <textarea id="ta-ro" readonly></textarea>
        </body></html>"#,
    );
    let engine = QueryEngine;

    let checks: &[(&str, &str, bool)] = &[
        ("req", ":required", true),
        ("req", ":optional", false),
        ("opt", ":required", false),
        ("opt", ":optional", true),
        ("ro", ":read-only", true),
        ("ro", ":read-write", false),
        ("rw", ":read-only", false),
        ("rw", ":read-write", true),
        ("ph", ":placeholder-shown", true),
        ("req", ":placeholder-shown", false),
        ("ta-req", ":required", true),
        ("ta-req", ":optional", false),
        ("ta-ro", ":read-only", true),
        ("ta-ro", ":read-write", false),
    ];

    for &(id, pseudo, expected) in checks {
        let got = engine
            .matches(&document, find_by_id(&document, id), pseudo)
            .unwrap_or_else(|e| panic!("{id} {pseudo}: {e:?}"));
        assert_eq!(got, expected, "{id} matches {pseudo}");
    }
}

#[test]
fn matches_checked() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <input id="cb-on"  type="checkbox" checked />
        <input id="cb-off" type="checkbox" />
        <input id="rb-on"  type="radio" checked />
        <input id="rb-off" type="radio" />
        <select id="sel">
            <option id="opt-def">A</option>
            <option id="opt-sel" selected>B</option>
        </select>
        </body></html>"#,
    );
    let engine = QueryEngine;

    let checks: &[(&str, bool)] = &[
        ("cb-on", true),
        ("cb-off", false),
        ("rb-on", true),
        ("rb-off", false),
        ("opt-sel", true),
        ("opt-def", false),
    ];
    for &(id, expected) in checks {
        let got = engine
            .matches(&document, find_by_id(&document, id), ":checked")
            .unwrap_or_else(|e| panic!("{id}: {e:?}"));
        assert_eq!(got, expected, "{id} :checked");
    }
}

#[test]
fn matches_validity_pseudos_detailed() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <!-- text input: required + empty → invalid -->
        <input id="txt-req-empty" type="text" required />
        <!-- text input: required + value → valid -->
        <input id="txt-req-fill" type="text" required value="hi" />
        <!-- text input: not required → valid regardless -->
        <input id="txt-opt" type="text" />
        <!-- checkbox: required + unchecked → invalid -->
        <input id="cb-req" type="checkbox" required />
        <!-- checkbox: required + checked → valid -->
        <input id="cb-req-on" type="checkbox" required checked />
        <!-- hidden: barred from constraint validation -->
        <input id="hidden-req" type="hidden" required />
        <!-- submit/image participate in constraint validation; reset is barred. -->
        <input id="submit-req" type="submit" required />
        <input id="image-req" type="image" required />
        <input id="reset-req"  type="reset"  required />
        <!-- select: required, no option selected with value → invalid -->
        <select id="sel-req-bad" required>
            <option value="">pick</option>
            <option value="x">X</option>
        </select>
        <!-- select: required, non-empty option selected → valid -->
        <select id="sel-req-good" required>
            <option value="">pick</option>
            <option value="x" selected>X</option>
        </select>
        <!-- select: not required → always valid -->
        <select id="sel-opt">
            <option value="">pick</option>
        </select>
        <!-- form: invalid if any descendant is invalid -->
        <form id="form-invalid">
            <input id="form-inp" required />
        </form>
        </body></html>"#,
    );
    let engine = QueryEngine;

    let checks: &[(&str, &str, bool)] = &[
        ("txt-req-empty", ":invalid", true),
        ("txt-req-empty", ":valid", false),
        ("txt-req-fill", ":valid", true),
        ("txt-req-fill", ":invalid", false),
        ("txt-opt", ":valid", true),
        ("txt-opt", ":invalid", false),
        ("cb-req", ":invalid", true),
        ("cb-req-on", ":valid", true),
        // Barred from constraint validation → matches neither :valid nor :invalid
        ("hidden-req", ":valid", false),
        ("hidden-req", ":invalid", false),
        ("submit-req", ":valid", true),
        ("submit-req", ":invalid", false),
        ("image-req", ":valid", true),
        ("image-req", ":invalid", false),
        ("reset-req", ":valid", false),
        ("reset-req", ":invalid", false),
        // Select validity
        ("sel-req-bad", ":invalid", true),
        ("sel-req-bad", ":valid", false),
        ("sel-req-good", ":valid", true),
        ("sel-req-good", ":invalid", false),
        ("sel-opt", ":valid", true),
        ("sel-opt", ":invalid", false),
        // Form: invalid because descendant is invalid
        ("form-invalid", ":invalid", true),
    ];

    for &(id, pseudo, expected) in checks {
        let got = engine
            .matches(&document, find_by_id(&document, id), pseudo)
            .unwrap_or_else(|e| panic!("{id} {pseudo}: {e:?}"));
        assert_eq!(got, expected, "{id} matches {pseudo}");
    }
}

#[test]
fn matches_lang_pseudo_class() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="en"    lang="en">English</div>
        <div id="en-us" lang="en-US">American English</div>
        <div id="zh"    lang="zh">Chinese</div>
        <div id="no-lang">No lang</div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    let checks: &[(&str, &str, bool)] = &[
        ("en", ":lang(en)", true),
        ("en-us", ":lang(en)", true), // prefix match
        ("en-us", ":lang(en-US)", true),
        ("zh", ":lang(zh)", true),
        ("zh", ":lang(en)", false),
        ("no-lang", ":lang(en)", false),
        // Case-insensitive tag matching
        ("en-us", ":lang(EN)", true),
    ];

    for &(id, pseudo, expected) in checks {
        let got = engine
            .matches(&document, find_by_id(&document, id), pseudo)
            .unwrap_or_else(|e| panic!("{id} {pseudo}: {e:?}"));
        assert_eq!(got, expected, "{id} matches {pseudo}");
    }
}

#[test]
fn matches_defined_for_html_elements() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="d"></div>
        <span id="s"></span>
        </body></html>"#,
    );
    let engine = QueryEngine;

    // All standard HTML elements are :defined in a headless context
    for id in ["d", "s"] {
        assert!(
            engine
                .matches(&document, find_by_id(&document, id), ":defined")
                .expect("should succeed"),
            "{id} should match :defined"
        );
    }
}

#[test]
fn scope_pseudo_class_in_all_in() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="root">
            <div id="child-a" class="item"></div>
            <div id="child-b" class="item">
                <span id="grandchild"></span>
            </div>
        </div>
        </body></html>"#,
    );
    let engine = QueryEngine;
    let root = find_by_id(&document, "root");

    // :scope > .item selects only direct children of root
    let direct = engine
        .query_selector_all_in(&document, root, ":scope > .item")
        .expect("should succeed");
    let ids: Vec<_> = direct
        .iter()
        .filter_map(|id| {
            document
                .node(*id)
                .and_then(|n| n.kind().as_element())
                .and_then(|el| el.attribute("id"))
        })
        .collect();
    assert_eq!(ids, vec!["child-a", "child-b"]);

    // :scope span selects grandchild
    let desc = engine
        .query_selector_all_in(&document, root, ":scope span")
        .expect("should succeed");
    assert_eq!(desc, vec![find_by_id(&document, "grandchild")]);
}

#[test]
fn matches_with_scope_uses_scope_root() {
    // #scope > #middle > #para: para is a descendant but not a direct child of scope.
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="scope">
            <div id="middle">
                <p id="para"></p>
            </div>
        </div>
        </body></html>"#,
    );
    let engine = QueryEngine;
    let scope = find_by_id(&document, "scope");
    let para = find_by_id(&document, "para");
    let middle = find_by_id(&document, "middle");

    // :scope p matches any descendant p
    assert!(
        engine
            .matches_with_scope(&document, para, ":scope p", scope)
            .expect("should succeed"),
        "para should match ':scope p' with scope=div#scope"
    );
    // :scope > p does NOT match para (middle div is between scope and para)
    assert!(
        !engine
            .matches_with_scope(&document, para, ":scope > p", scope)
            .expect("should succeed"),
        "para should NOT match ':scope > p' (middle div is between scope and para)"
    );
    // :scope > div matches middle since it IS a direct child of scope
    assert!(
        engine
            .matches_with_scope(&document, middle, ":scope > div", scope)
            .expect("should succeed"),
        "middle should match ':scope > div' as direct child of scope"
    );
    // middle does NOT match when scoped to para (para has no div children)
    assert!(
        !engine
            .matches_with_scope(&document, middle, ":scope > div", para)
            .expect("should succeed"),
        "middle should NOT match ':scope > div' when scoped to para"
    );
}
