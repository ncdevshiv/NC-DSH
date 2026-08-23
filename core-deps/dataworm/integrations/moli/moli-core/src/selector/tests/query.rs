use super::{QueryEngine, find_by_id, host_find_by_id, parse_document};
use crate::dom::{
    NodeId,
    native::{DomHost, NativeDom},
};

#[test]
fn matches_query_selector_all_core_subset_without_v8() {
    let document = parse_document(
        r#"
        <!doctype html>
        <html>
          <body>
            <div id="app" class="container active" data-role="main">
              <p class="item">First</p>
              <p class="item special">Second</p>
              <section class="nested">
                <span id="target" class="item">Third</span>
              </section>
            </div>
            <div id="outside" class="item">Outside</div>
          </body>
        </html>
        "#,
    );
    let engine = QueryEngine;

    let class_matches = engine
        .query_selector_all(&document, ".item")
        .expect("query should succeed");
    let class_ids = class_matches
        .iter()
        .map(|node_id| {
            document
                .node(*node_id)
                .and_then(|node| node.kind().as_element())
                .and_then(|el| el.attribute("id"))
                .unwrap_or("")
        })
        .collect::<Vec<_>>();
    assert_eq!(class_ids, vec!["", "", "target", "outside"]);

    let descendant = engine
        .query_selector(&document, "div section > span.item")
        .expect("query should succeed")
        .expect("expected descendant match");
    assert_eq!(descendant, find_by_id(&document, "target"));

    let selector_list = engine
        .query_selector_all(&document, "#target, #outside")
        .expect("query should succeed");
    assert_eq!(
        selector_list,
        vec![
            find_by_id(&document, "target"),
            find_by_id(&document, "outside")
        ]
    );
}

#[test]
fn matches_scope_matches_and_closest_within_subtree() {
    let document = parse_document(
        r#"
        <!doctype html>
        <html>
          <body>
            <div id="root">
              <div id="child1" class="item">
                <span id="grandchild1">Grandchild 1</span>
                <span id="grandchild2">Grandchild 2</span>
              </div>
              <div id="child2" class="item">
                <span id="grandchild3">Grandchild 3</span>
              </div>
            </div>
          </body>
        </html>
        "#,
    );
    let engine = QueryEngine;
    let root = find_by_id(&document, "root");
    let grandchild3 = find_by_id(&document, "grandchild3");

    let scoped = engine
        .query_selector_all_in(&document, root, ":scope > div")
        .expect("scoped query should succeed");
    assert_eq!(
        scoped,
        vec![
            find_by_id(&document, "child1"),
            find_by_id(&document, "child2")
        ]
    );

    assert!(
        engine
            .matches_with_scope(&document, grandchild3, ":scope span", root)
            .expect("match should succeed")
    );
    assert_eq!(
        engine
            .closest(&document, grandchild3, ".item")
            .expect("closest should succeed"),
        Some(find_by_id(&document, "child2"))
    );
}

#[test]
fn scoped_query_selector_scope_does_not_return_scope_element_itself() {
    let document = parse_document(
        r#"<!doctype html>
        <html>
          <body>
            <div id="container">
              <div id="child1" class="item"><span id="grandchild1"></span></div>
              <div id="child2" class="item"><span id="grandchild2"></span></div>
            </div>
          </body>
        </html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;
    let container = find_by_id(&document, "container");

    assert_eq!(
        engine
            .query_selector_in(&document, container, ":scope")
            .expect("tree scoped query should succeed"),
        None
    );
    assert_eq!(
        engine
            .query_selector_in_host(&host, container, ":scope")
            .expect("host scoped query should succeed"),
        None
    );

    assert!(
        engine
            .query_selector_all_in(&document, container, ":scope")
            .expect("tree scoped query-all should succeed")
            .is_empty()
    );
    assert!(
        engine
            .query_selector_all_in_host(&host, container, ":scope")
            .expect("host scoped query-all should succeed")
            .is_empty()
    );
}

#[test]
fn scoped_query_selector_supports_id_and_sibling_patterns_from_servo_wpt() {
    let document = parse_document(
        r#"<!doctype html>
        <html>
          <body>
            <div id="root">
              <h1 id="test"></h1>
              <p id="target"><span>hello</span></p>
            </div>
          </body>
        </html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;
    let root = find_by_id(&document, "root");
    let target = find_by_id(&document, "target");

    assert_eq!(
        engine
            .query_selector_in(&document, root, ":scope > p")
            .expect("tree scoped query should succeed"),
        Some(target)
    );
    assert_eq!(
        engine
            .query_selector_in_host(&host, root, ":scope > p")
            .expect("host scoped query should succeed"),
        Some(target)
    );
    assert_eq!(
        engine
            .query_selector_in(&document, root, "#test + p")
            .expect("tree sibling query should succeed"),
        Some(target)
    );
    assert_eq!(
        engine
            .query_selector_in_host(&host, root, "#test + p")
            .expect("host sibling query should succeed"),
        Some(target)
    );
    assert!(
        engine
            .query_selector_all_in(&document, target, "#test + p")
            .expect("tree nested sibling query should succeed")
            .is_empty()
    );
    assert!(
        engine
            .query_selector_all_in_host(&host, target, "#test + p")
            .expect("host nested sibling query should succeed")
            .is_empty()
    );
}

#[test]
fn closest_supports_scope_relative_selectors_from_servo_wpt() {
    let document = parse_document(
        r#"<!doctype html>
        <html>
          <body>
            <div id="outer">
              <select id="select">
                <option id="option-a">A</option>
                <option id="option-b">B</option>
              </select>
            </div>
          </body>
        </html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;
    let option_a = find_by_id(&document, "option-a");
    let select = find_by_id(&document, "select");

    assert_eq!(
        engine
            .closest(&document, option_a, ":scope")
            .expect("tree closest should succeed"),
        Some(option_a)
    );
    assert_eq!(
        engine
            .closest_host(&host, option_a, ":scope")
            .expect("host closest should succeed"),
        Some(option_a)
    );
    assert_eq!(
        engine
            .closest(&document, option_a, "select > :scope")
            .expect("tree closest should succeed"),
        Some(option_a)
    );
    assert_eq!(
        engine
            .closest_host(&host, option_a, "select > :scope")
            .expect("host closest should succeed"),
        Some(option_a)
    );
    assert_eq!(
        engine
            .closest(&document, option_a, "div > :scope")
            .expect("tree closest should succeed"),
        None
    );
    assert_eq!(
        engine
            .closest_host(&host, option_a, "div > :scope")
            .expect("host closest should succeed"),
        None
    );
    assert_eq!(
        engine
            .closest(&document, select, ":scope")
            .expect("tree closest should succeed"),
        Some(select)
    );
}

#[test]
fn closest_covers_more_servo_wpt_relationships_and_pseudos() {
    let document = parse_document(
        r#"<!doctype html>
        <html>
          <body id="body">
            <div id="test8" class="div3" style="display:none">
              <div id="test7" class="div2">
                <div id="test6" class="div1">
                  <form id="test10" class="form2"></form>
                  <form id="test5" class="form1" name="form-a">
                    <input id="test1" class="input1" required>
                    <fieldset class="fieldset2" id="test2">
                      <select id="test3" class="select1" required>
                        <option default id="test4" value="">Test4</option>
                        <option selected id="test11">Test11</option>
                        <option id="test12">Test12</option>
                        <option id="test13">Test13</option>
                      </select>
                      <input id="test9" type="text" required>
                    </fieldset>
                  </form>
                </div>
              </div>
            </div>
          </body>
        </html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;

    let cases: &[(&str, &str, Option<&str>)] = &[
        ("test12", "select", Some("test3")),
        ("test13", "fieldset", Some("test2")),
        ("test13", "div", Some("test6")),
        ("test3", "body", Some("body")),
        ("test4", "[default]", Some("test4")),
        ("test11", "[selected]", Some("test11")),
        ("test4", "[selected]", None),
        ("test12", r#"[name="form-a"]"#, Some("test5")),
        ("test13", r#"form[name="form-a"]"#, Some("test5")),
        ("test9", "input[required]", Some("test9")),
        ("test9", "select[required]", None),
        ("test13", "div:not(.div1)", Some("test7")),
        ("test6", "div.div3", Some("test8")),
        ("test1", "div#test7", Some("test7")),
        ("test12", ".div3 > .div2", Some("test7")),
        ("test12", ".div3 > .div1", None),
        ("test9", "form > input[required]", None),
        ("test12", "fieldset > select[required]", Some("test3")),
        ("test6", "input + fieldset", None),
        ("test3", "form + form", Some("test5")),
        ("test5", "form + form", Some("test5")),
        ("test10", ":empty", Some("test10")),
        ("test11", ":last-child", Some("test2")),
        ("test12", ":first-child", Some("test3")),
    ];

    for &(start_id, selector, expected_id) in cases {
        let start = find_by_id(&document, start_id);
        let expected = expected_id.map(|id| find_by_id(&document, id));

        assert_eq!(
            engine
                .closest(&document, start, selector)
                .unwrap_or_else(|e| panic!(
                    "tree closest({selector:?}) from {start_id} failed: {e:?}"
                )),
            expected,
            "tree closest({selector:?}) from {start_id}"
        );
        assert_eq!(
            engine
                .closest_host(&host, start, selector)
                .unwrap_or_else(|e| panic!(
                    "host closest({selector:?}) from {start_id} failed: {e:?}"
                )),
            expected,
            "host closest({selector:?}) from {start_id}"
        );
    }
}

#[test]
fn query_engine_host_entrypoints_match_tree_entrypoints() {
    let document = parse_document(
        r#"<!doctype html>
        <html>
          <body>
            <div id="root" class="scope">
              <div id="child-a" class="item">
                <span id="leaf-a" data-kind="alpha"></span>
              </div>
              <div id="child-b" class="item">
                <span id="leaf-b" data-kind="beta"></span>
              </div>
            </div>
          </body>
        </html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;
    let root = find_by_id(&document, "root");
    let leaf_b = find_by_id(&document, "leaf-b");
    let child_b = find_by_id(&document, "child-b");

    assert_eq!(
        engine
            .query_selector_all(&document, ".item")
            .expect("tree query should succeed"),
        engine
            .query_selector_all_host(&host, ".item")
            .expect("host query should succeed")
    );

    assert_eq!(
        engine
            .query_selector_all_in(&document, root, ":scope > .item")
            .expect("tree scoped query should succeed"),
        engine
            .query_selector_all_in_host(&host, root, ":scope > .item")
            .expect("host scoped query should succeed")
    );

    assert_eq!(
        engine
            .query_selector(&document, r#"span[data-kind="beta"]"#)
            .expect("tree query should succeed"),
        engine
            .query_selector_host(&host, r#"span[data-kind="beta"]"#)
            .expect("host query should succeed")
    );

    assert_eq!(
        engine
            .matches(&document, leaf_b, ".item span")
            .expect("tree matches should succeed"),
        engine
            .matches_host(&host, leaf_b, ".item span")
            .expect("host matches should succeed")
    );

    assert_eq!(
        engine
            .matches_with_scope(&document, leaf_b, ":scope span", root)
            .expect("tree scoped matches should succeed"),
        engine
            .matches_with_scope_host(&host, leaf_b, ":scope span", root)
            .expect("host scoped matches should succeed")
    );

    assert_eq!(
        engine
            .closest(&document, leaf_b, ".item")
            .expect("tree closest should succeed"),
        engine
            .closest_host(&host, leaf_b, ".item")
            .expect("host closest should succeed")
    );

    assert_eq!(
        engine
            .closest_host(&host, child_b, "#root")
            .expect("host closest should succeed"),
        Some(root)
    );
}

#[test]
fn query_engine_host_scoped_matches_handles_invalid_scope_inputs() {
    let document = parse_document(
        r#"<!doctype html>
        <html>
          <body>
            <div id="root"><span id="leaf"></span></div>
          </body>
        </html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;
    let document_handle = host.document_handle();
    let leaf = find_by_id(&document, "leaf");

    assert!(
        !engine
            .matches_with_scope_host(&host, leaf, ":scope span", document_handle)
            .expect("document scope should return false instead of failing")
    );
    assert!(
        !engine
            .matches_with_scope_host(
                &host,
                document_handle,
                ":scope span",
                find_by_id(&document, "root")
            )
            .expect("non-element subject should return false instead of failing")
    );
}

#[test]
fn scoped_query_selector_handles_document_fragment_roots() {
    let mut document = parse_document("<!doctype html><html><body></body></html>");
    let fragment = document.create_document_fragment();
    let section = document.create_element("section");
    let nested = document.create_element("span");
    assert!(document.append_child(fragment, section));
    assert!(document.append_child(section, nested));

    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;

    assert_eq!(
        engine
            .query_selector_in(&document, fragment, "section")
            .expect("tree fragment query should succeed"),
        Some(section)
    );
    assert_eq!(
        engine
            .query_selector_in_host(&host, fragment, "section")
            .expect("host fragment query should succeed"),
        Some(section)
    );
    assert_eq!(
        engine
            .query_selector_all_in_host(&host, fragment, "*")
            .expect("host fragment query all should succeed"),
        vec![section, nested]
    );
}

#[test]
fn query_selector_html_tag_names_are_ascii_case_insensitive() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <h1 id="heading-1">Heading 1</h1>
        <h2 id="heading-2">Heading 2</h2>
        <nav id="nav-1">Navigation</nav>
        </body></html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;

    let cases: &[(&str, Vec<&str>)] = &[
        ("h1", vec!["heading-1"]),
        ("H1", vec!["heading-1"]),
        ("h2", vec!["heading-2"]),
        ("H2", vec!["heading-2"]),
        ("nav", vec!["nav-1"]),
        ("NAV", vec!["nav-1"]),
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
fn query_selector_supports_non_ascii_and_escaped_identifiers() {
    let document = parse_document(
        "<!doctype html><html><body>\n        <div id=\"ascii\" class=\"caf\u{00e9}\">Non-ASCII class 1</div>\n        <div id=\"jp\" class=\"\u{65E5}\u{672C}\u{8A9E}\">Non-ASCII class 2</div>\n        <span id=\"ni\u{00f1}o\">Non-ASCII ID 1</span>\n        <p id=\"\u{1F3A8}\">Non-ASCII ID 2</p>\n        <span id=\".,:!\">Punctuation test</span>\n        </body></html>",
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;

    let cases: &[(&str, Option<&str>)] = &[
        (".caf\u{00e9}", Some("ascii")),
        (".\u{65E5}\u{672C}\u{8A9E}", Some("jp")),
        ("#ni\u{00f1}o", Some("ni\u{00f1}o")),
        ("#\u{1F3A8}", Some("\u{1F3A8}")),
        ("div.caf\u{00e9}", Some("ascii")),
        ("span#ni\u{00f1}o", Some("ni\u{00f1}o")),
        (r#"#\.\,\:\!"#, Some(".,:!")),
    ];

    for &(selector, expected_id) in cases {
        let expected = expected_id.map(|id| find_by_id(&document, id));
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
fn query_selector_supports_more_css_escape_forms_from_servo_wpt() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <span id="0nextIsWhiteSpace"></span>
        <span id="0nextIsNotHexLetters"></span>
        <span id=".comma"></span>
        <span id="-minus"></span>
        <span id="hello"></span>
        <span id="&amp;B"></span>
        <span id=".,:!"></span>
        </body></html>"#,
    );
    let host = DomHost::from_dom(document.clone());
    let engine = QueryEngine;

    let cases: &[(&str, &str)] = &[
        (r#"#\30 nextIsWhiteSpace"#, "0nextIsWhiteSpace"),
        (r#"#\30nextIsNotHexLetters"#, "0nextIsNotHexLetters"),
        (r#"#\.comma"#, ".comma"),
        (r#"#\-minus"#, "-minus"),
        (r#"#hel\6Co"#, "hello"),
        (r#"#\26 B"#, "&B"),
        (r#"#\.\,\:\!"#, ".,:!"),
    ];

    for &(selector, expected_id) in cases {
        let expected = Some(find_by_id(&document, expected_id));
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
fn query_selector_all_excludes_removed_elements_after_dom_mutation() {
    fn append_test_link_native(
        document: &mut NativeDom,
        parent: NodeId,
        id: &str,
        with_img: bool,
    ) -> NodeId {
        let link = document.create_element("a");
        assert!(document.set_attribute(link, "id", id));
        assert!(document.set_attribute(link, "class", "test"));
        if with_img {
            let img = document.create_element("img");
            assert!(document.set_attribute(img, "src", "foo.jpg"));
            assert!(document.append_child(link, img));
        }
        assert!(document.append_child(parent, link));
        link
    }

    fn clear_children_native(document: &mut NativeDom, parent: NodeId) {
        let children = document.child_nodes(parent).unwrap_or_default();
        for child in children {
            assert!(document.remove_child(parent, child));
        }
    }

    fn append_test_link_host(
        host: &mut DomHost,
        parent: NodeId,
        id: &str,
        with_img: bool,
    ) -> NodeId {
        let link = host.create_element("a");
        assert!(host.set_attribute(link, "id", id));
        assert!(host.set_attribute(link, "class", "test"));
        if with_img {
            let img = host.create_element("img");
            assert!(host.set_attribute(img, "src", "foo.jpg"));
            assert!(host.append_child(link, img));
        }
        assert!(host.append_child(parent, link));
        link
    }

    fn clear_children_host(host: &mut DomHost, parent: NodeId) {
        let children = host.child_nodes(parent).unwrap_or_default();
        for child in children {
            assert!(host.remove_child(parent, child));
        }
    }

    let mut document =
        parse_document(r#"<!doctype html><html><body><div id="container"></div></body></html>"#);
    let engine = QueryEngine;
    let container = find_by_id(&document, "container");

    append_test_link_native(&mut document, container, "link-a", false);
    assert_eq!(
        engine
            .query_selector_all_in(&document, container, "a.test")
            .expect("tree initial query should succeed"),
        vec![find_by_id(&document, "link-a")]
    );

    clear_children_native(&mut document, container);
    append_test_link_native(&mut document, container, "link-b", true);
    assert_eq!(
        engine
            .query_selector_all_in(&document, container, "a.test")
            .expect("tree replacement query should succeed"),
        vec![find_by_id(&document, "link-b")]
    );

    clear_children_native(&mut document, container);
    append_test_link_native(&mut document, container, "link-a", false);
    let reverted_tree = engine
        .query_selector_all_in(&document, container, "a.test")
        .expect("tree reverted query should succeed");
    assert_eq!(reverted_tree.len(), 1);
    let reverted_tree_match = reverted_tree[0];
    assert_eq!(document.parent_node(reverted_tree_match), Some(container));
    assert_eq!(
        document
            .element(reverted_tree_match)
            .and_then(|el| el.attribute("id")),
        Some("link-a")
    );

    let host_document =
        parse_document(r#"<!doctype html><html><body><div id="container"></div></body></html>"#);
    let mut host = DomHost::from_dom(host_document);
    let host_container = host_find_by_id(&host, "container");

    append_test_link_host(&mut host, host_container, "link-a", false);
    assert_eq!(
        engine
            .query_selector_all_in_host(&host, host_container, "a.test")
            .expect("host initial query should succeed"),
        vec![host_find_by_id(&host, "link-a")]
    );

    clear_children_host(&mut host, host_container);
    append_test_link_host(&mut host, host_container, "link-b", true);
    assert_eq!(
        engine
            .query_selector_all_in_host(&host, host_container, "a.test")
            .expect("host replacement query should succeed"),
        vec![host_find_by_id(&host, "link-b")]
    );

    clear_children_host(&mut host, host_container);
    append_test_link_host(&mut host, host_container, "link-a", false);
    let reverted_host = engine
        .query_selector_all_in_host(&host, host_container, "a.test")
        .expect("host reverted query should succeed");
    assert_eq!(reverted_host.len(), 1);
    let reverted_host_match = reverted_host[0];
    assert_eq!(
        host.node(reverted_host_match)
            .and_then(|node| node.parent_node()),
        Some(host_container)
    );
    assert_eq!(
        host.get_attribute(reverted_host_match, "id").as_deref(),
        Some("link-a")
    );
}

#[test]
fn selector_list_results_in_document_order() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="first"></div>
        <p id="second"></p>
        <span id="third"></span>
        <div id="fourth"></div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    // Results must be in document order regardless of selector list order.
    let results = engine
        .query_selector_all(&document, "#fourth, #second, #first, #third")
        .expect("should succeed");
    let ids: Vec<_> = results
        .iter()
        .map(|id| {
            document
                .node(*id)
                .and_then(|n| n.kind().as_element())
                .and_then(|el| el.attribute("id"))
                .unwrap_or("")
        })
        .collect();
    assert_eq!(ids, vec!["first", "second", "third", "fourth"]);
}

#[test]
fn query_selector_returns_first_match() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="a" class="target"></div>
        <div id="b" class="target"></div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    let first = engine
        .query_selector(&document, ".target")
        .expect("should succeed")
        .expect("should find element");
    assert_eq!(first, find_by_id(&document, "a"));
}

#[test]
fn query_selector_in_restricts_to_subtree() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="scope">
            <p id="inner1" class="item">inner 1</p>
            <p id="inner2" class="item">inner 2</p>
        </div>
        <p id="outer" class="item">outer</p>
        </body></html>"#,
    );
    let engine = QueryEngine;
    let scope = find_by_id(&document, "scope");

    let all_in = engine
        .query_selector_all_in(&document, scope, ".item")
        .expect("should succeed");
    let ids: Vec<_> = all_in
        .iter()
        .map(|id| {
            document
                .node(*id)
                .and_then(|n| n.kind().as_element())
                .and_then(|el| el.attribute("id"))
                .unwrap_or("")
        })
        .collect();
    // Must NOT include #outer
    assert_eq!(ids, vec!["inner1", "inner2"]);

    // query_selector_in returns the first within scope
    let first = engine
        .query_selector_in(&document, scope, ".item")
        .expect("should succeed")
        .expect("should find one");
    assert_eq!(first, find_by_id(&document, "inner1"));
}

#[test]
fn closest_walks_ancestors() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="grandparent" class="wrap">
            <div id="parent">
                <span id="child">leaf</span>
            </div>
        </div>
        </body></html>"#,
    );
    let engine = QueryEngine;
    let child = find_by_id(&document, "child");

    // closest on self
    let span = engine
        .closest(&document, child, "span")
        .expect("should succeed");
    assert_eq!(span, Some(find_by_id(&document, "child")));

    // ancestor class
    let wrap = engine
        .closest(&document, child, ".wrap")
        .expect("should succeed");
    assert_eq!(wrap, Some(find_by_id(&document, "grandparent")));

    // no ancestor matches
    let no_match = engine
        .closest(&document, child, "table")
        .expect("should succeed");
    assert_eq!(no_match, None);
}

#[test]
fn matches_element_level_api() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="target" class="box active" data-role="main"></div>
        <div id="other" class="box"></div>
        </body></html>"#,
    );
    let engine = QueryEngine;
    let target = find_by_id(&document, "target");

    let checks: &[(&str, bool)] = &[
        ("div", true),
        (".box", true),
        (".box.active", true),
        (".active", true),
        (".inactive", false),
        ("#target", true),
        ("#other", false),
        (r#"[data-role="main"]"#, true),
        (r#"[data-role="other"]"#, false),
        ("div.box.active", true),
        ("span", false),
        ("div, span", true), // selector list: div matches
    ];

    for &(sel, expected) in checks {
        let got = engine
            .matches(&document, target, sel)
            .unwrap_or_else(|e| panic!("{sel}: {e:?}"));
        assert_eq!(got, expected, "matches({sel:?})");
    }
}
