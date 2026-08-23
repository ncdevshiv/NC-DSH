use std::time::Instant;

use moli_dom::native::{DomHost, NativeDom};
use url::Url;

const ELEMENTS: usize = 2_000;
const DENSE_ITERATIONS: usize = 100;
const SPARSE_ITERATIONS: usize = 20_000;
const NAMESPACE: &str = "urn:moli:element-query-bench";

fn main() {
    let mut host = DomHost::from_dom(NativeDom::new_html(
        Url::parse("https://element-query-bench.test/").expect("valid benchmark URL"),
    ));
    let document = host.document_handle();
    for _ in 0..ELEMENTS {
        let element = host
            .create_element_ns(Some(NAMESPACE), "bench:item")
            .expect("valid benchmark element");
        assert!(host.append_child(document, element));
    }
    let rare = host
        .create_element_ns(Some(NAMESPACE), "bench:rare")
        .expect("valid rare benchmark element");
    assert!(host.append_child(document, rare));

    assert_eq!(
        host.elements_by_tag_name_ns(document, Some(NAMESPACE), "item", true)
            .len(),
        ELEMENTS
    );

    let dense_start = Instant::now();
    let mut checksum = 0;
    for _ in 0..DENSE_ITERATIONS {
        checksum += host
            .elements_by_tag_name_ns(document, Some(NAMESPACE), "item", true)
            .len();
    }
    let dense_elapsed = dense_start.elapsed();

    let sparse_start = Instant::now();
    for _ in 0..SPARSE_ITERATIONS {
        checksum += host
            .elements_by_tag_name_ns(document, Some(NAMESPACE), "rare", true)
            .len();
    }
    let sparse_elapsed = sparse_start.elapsed();

    assert_eq!(checksum, ELEMENTS * DENSE_ITERATIONS + SPARSE_ITERATIONS);
    println!(
        "elements={ELEMENTS} dense_iterations={DENSE_ITERATIONS} sparse_iterations={SPARSE_ITERATIONS} dense_ms={} sparse_ms={}",
        dense_elapsed.as_millis(),
        sparse_elapsed.as_millis(),
    );
}
