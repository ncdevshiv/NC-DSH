use super::{QueryEngine, host_find_by_id, host_from_html};

#[test]
fn host_query_engine_handles_scoped_queries_and_hot_script_selectors() {
    let host = host_from_html(
        r#"<!doctype html>
        <html>
          <body>
            <div id="root">
              <script id="boot" src="/_next/static/chunks/main.js"></script>
              <div id="nested">
                <script id="chunk-a" src="/_next/static/chunks/44df0075bd47eed8.js"></script>
                <script id="chunk-b" src="/_next/static/chunks/1ae5472e733eb2d2.js?dpl=abc"></script>
              </div>
            </div>
          </body>
        </html>"#,
    );
    let engine = QueryEngine;
    let root = host_find_by_id(&host, "root");
    let nested = host_find_by_id(&host, "nested");
    let chunk_a = host_find_by_id(&host, "chunk-a");
    let chunk_b = host_find_by_id(&host, "chunk-b");

    let direct_scripts = engine
        .query_selector_all_in_host(&host, root, ":scope > script")
        .expect("scoped script query should succeed");
    assert_eq!(direct_scripts, vec![host_find_by_id(&host, "boot")]);

    let chunk_scripts = engine
        .query_selector_all_host(&host, r#"script[src^="/_next/static/chunks/"]"#)
        .expect("global hot-path script query should succeed");
    assert_eq!(
        chunk_scripts,
        vec![
            host_find_by_id(&host, "boot"),
            host_find_by_id(&host, "chunk-a"),
            host_find_by_id(&host, "chunk-b")
        ]
    );

    let exact_chunk = engine
        .query_selector_host(
            &host,
            r#"script[src="/_next/static/chunks/44df0075bd47eed8.js"]"#,
        )
        .expect("exact script src query should succeed");
    assert_eq!(exact_chunk, Some(chunk_a));

    let selector_list_chunk = engine
        .query_selector_all_host(
            &host,
            r#"script[src="/_next/static/chunks/44df0075bd47eed8.js"],script[src^="/_next/static/chunks/1ae5472e733eb2d2.js?"]"#,
        )
        .expect("selector list hot-path script query should succeed");
    assert_eq!(selector_list_chunk, vec![chunk_a, chunk_b]);

    let stylesheet = host_from_html(
        r#"<!doctype html>
        <html>
          <head>
            <link rel="preload" as="style" href="/_next/static/chunks/site.css" />
            <link rel="stylesheet" href="/_next/static/chunks/site.css" />
          </head>
          <body></body>
        </html>"#,
    );
    assert!(
        engine
            .query_selector_host(
                &stylesheet,
                r#"link[rel="preload"][as="style"][href="/_next/static/chunks/site.css"]"#,
            )
            .expect("compound link query should succeed")
            .is_some()
    );

    assert!(
        engine
            .matches_with_scope_host(&host, chunk_b, ":scope script[src*=\"1ae5472e\"]", nested)
            .expect("scoped matches should succeed")
    );

    assert_eq!(
        engine
            .closest_host(&host, chunk_b, "div")
            .expect("closest should succeed"),
        Some(nested)
    );
}

#[test]
fn host_query_engine_runs_basic_dom_queries() {
    let host = host_from_html(
        r#"
        <!doctype html>
        <html>
          <body>
            <div id="root">
              <script id="first" src="/_next/static/chunks/a.js"></script>
              <script id="second" src="/_next/static/chunks/b.js"></script>
              <section class="group">
                <span id="leaf" class="target">Leaf</span>
              </section>
            </div>
          </body>
        </html>
        "#,
    );
    let engine = QueryEngine;

    let scripts = engine
        .query_selector_all_host(&host, r#"script[src^="/_next/static/chunks/"]"#)
        .expect("adapter query_selector_all should succeed");
    assert_eq!(scripts.len(), 2);

    let leaf = engine
        .query_selector_host(&host, "section.group > span.target")
        .expect("adapter query_selector should succeed")
        .expect("adapter query_selector should find a match");
    assert_eq!(
        host.node(leaf)
            .and_then(|node| node.as_element())
            .and_then(|element| element.attribute("id")),
        Some("leaf")
    );

    assert!(
        engine
            .matches_host(&host, leaf, "span.target")
            .expect("adapter matches should succeed")
    );
    assert_eq!(
        engine
            .closest_host(&host, leaf, "#root")
            .expect("adapter closest should succeed"),
        Some(
            engine
                .query_selector_host(&host, "#root")
                .expect("query_selector should succeed")
                .expect("expected root match")
        )
    );
}

#[test]
fn host_adapter_matches_live_validity_and_indeterminate_state() {
    let mut host = host_from_html(
        r#"<!doctype html><html><body>
        <form id="form">
            <input id="req" type="text" required />
            <input id="cb" type="checkbox" />
        </form>
        </body></html>"#,
    );
    let engine = QueryEngine;
    let req = host_find_by_id(&host, "req");
    let cb = host_find_by_id(&host, "cb");
    let form = host_find_by_id(&host, "form");

    assert!(
        engine
            .matches_host(&host, req, ":invalid")
            .expect("host :invalid should succeed")
    );
    assert!(
        engine
            .matches_host(&host, form, ":invalid")
            .expect("host form :invalid should succeed")
    );
    assert!(
        !engine
            .matches_host(&host, cb, ":indeterminate")
            .expect("host :indeterminate should succeed")
    );

    assert!(host.set_input_value(req, "ok"));
    assert!(
        engine
            .matches_host(&host, req, ":valid")
            .expect("host :valid after value set should succeed")
    );
    assert!(
        !engine
            .matches_host(&host, form, ":invalid")
            .expect("host form :invalid after value set should succeed")
    );

    assert!(host.set_indeterminate_state(cb, true));
    assert!(
        engine
            .matches_host(&host, cb, ":indeterminate")
            .expect("host :indeterminate after toggle should succeed")
    );
}
