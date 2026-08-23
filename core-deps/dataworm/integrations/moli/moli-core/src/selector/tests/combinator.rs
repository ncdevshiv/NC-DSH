use super::{QueryEngine, assert_query_ids, parse_document};

#[test]
fn matches_combinators() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="parent">
            <p id="child1" class="item">C1</p>
            <span id="child2">C2</span>
            <p id="child3" class="item">C3</p>
            <div id="nested">
                <p id="grandchild">GC</p>
            </div>
        </div>
        <p id="sibling-of-parent">Sibling</p>
        </body></html>"#,
    );
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            // direct children only
            ("#parent > p", vec!["child1", "child3"]),
            ("#parent > div", vec!["nested"]),
            // grandchild IS a descendant but NOT a direct child of #parent
            ("#parent > #grandchild", vec![]),
            ("#parent #grandchild", vec!["grandchild"]),
            // adjacent sibling: only the immediately next one
            ("#child1 + span", vec!["child2"]),
            ("#child1 + p", vec![]), // child2 (span) is between
            ("#child2 + p", vec!["child3"]),
            // general sibling: any subsequent
            ("#child1 ~ p", vec!["child3"]),
            ("#child1 ~ span", vec!["child2"]),
            ("#child1 ~ div", vec!["nested"]),
        ],
    );
}

#[test]
fn matches_type_selector() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="d1"></div>
        <span id="s1"></span>
        <div id="d2"></div>
        <p id="p1"></p>
        </body></html>"#,
    );
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            ("div", vec!["d1", "d2"]),
            ("span", vec!["s1"]),
            ("p", vec!["p1"]),
            ("article", vec![]),
        ],
    );
}

#[test]
fn matches_universal_selector() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="a"><span id="b"></span></div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    let all = engine
        .query_selector_all(&document, "*")
        .expect("should succeed");
    // * selects every element: html, head (implicit), body, div#a, span#b
    assert!(
        all.len() >= 4,
        "expected at least 4 elements, got {}",
        all.len()
    );
    // Ensure div#a and span#b are both in the result
    assert!(all.contains(&super::find_by_id(&document, "a")));
    assert!(all.contains(&super::find_by_id(&document, "b")));
}

#[test]
fn matches_class_selectors() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <div id="one" class="alpha"></div>
        <div id="two" class="alpha beta"></div>
        <div id="three" class="beta gamma"></div>
        <div id="four" class="alpha beta gamma"></div>
        </body></html>"#,
    );
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            (".alpha", vec!["one", "two", "four"]),
            (".beta", vec!["two", "three", "four"]),
            (".gamma", vec!["three", "four"]),
            (".alpha.beta", vec!["two", "four"]),
            (".alpha.beta.gamma", vec!["four"]),
            (".delta", vec![]),
        ],
    );
}

#[test]
fn nth_child_with_sibling_combinator() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <ul id="ul">
            <li id="a">1</li>
            <li id="b">2</li>
            <li id="c">3</li>
            <li id="d">4</li>
        </ul>
        </body></html>"#,
    );
    let engine = QueryEngine;

    // li:nth-child(2) ~ li selects all li siblings after position 2
    assert_query_ids(
        &engine,
        &document,
        &[
            ("#ul li:nth-child(2) ~ li", vec!["c", "d"]),
            ("#ul li:nth-child(odd)", vec!["a", "c"]),
            ("#ul li:nth-child(even)", vec!["b", "d"]),
        ],
    );
}

#[test]
fn compound_selectors_mixed_conditions() {
    let document = parse_document(
        r#"<!doctype html><html><body>
        <input id="a" type="text" class="fancy" required value="hi" />
        <input id="b" type="text" class="fancy" required />
        <input id="c" type="text" required value="ok" />
        <input id="d" type="checkbox" class="fancy" checked />
        </body></html>"#,
    );
    let engine = QueryEngine;

    assert_query_ids(
        &engine,
        &document,
        &[
            // text input, fancy class, required, but only VALID ones
            (r#"input[type="text"].fancy:required:valid"#, vec!["a"]),
            // required text inputs that are invalid
            (r#"input[type="text"]:required:invalid"#, vec!["b"]),
            // checked checkboxes with fancy class
            (r#"input[type="checkbox"].fancy:checked"#, vec!["d"]),
        ],
    );
}
