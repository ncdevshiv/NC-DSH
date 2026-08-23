//! Closure guard for the renderer-neutral callback kernel itself.
//!
//! The kernel may temporarily root one dynamically resolved callback-interface
//! operation, but it delegates the actual V8 call to a renderer adapter. It
//! must not grow a hidden invocation, scheduler, or browser-owner policy.

use std::{collections::BTreeMap, fs, path::Path};

#[test]
fn callback_kernel_raw_function_boundary_is_frozen() {
    assert_eq!(
        production_source_inventory(count_raw_global_functions),
        BTreeMap::from([("invocation.rs".to_owned(), 1)]),
        "the callback kernel may only root the one dynamically resolved interface operation"
    );
}

#[test]
fn callback_kernel_contains_no_direct_v8_call() {
    assert!(
        production_source_inventory(count_direct_v8_calls).is_empty(),
        "the callback kernel must delegate V8 invocation to the renderer-owned adapter"
    );
}

fn production_source_inventory(count: fn(&str) -> usize) -> BTreeMap<String, usize> {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    fs::read_dir(&source_root)
        .expect("callback crate source directory should exist")
        .map(|entry| entry.expect("callback crate source entry should be readable"))
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return None;
            }
            let file_name = path.file_name()?.to_str()?;
            if matches!(file_name, "source_boundary_tests.rs" | "tests.rs") {
                return None;
            }
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            let matches = count(&source);
            (matches > 0).then(|| (file_name.to_owned(), matches))
        })
        .collect()
}

fn compact_source(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn count_raw_global_functions(source: &str) -> usize {
    let source = compact_source(source);
    source.matches(concat!("Global<", "v8::Function>")).count()
        + source.matches(concat!("Global<", "Function>")).count()
}

fn count_direct_v8_calls(source: &str) -> usize {
    let source = compact_source(source);
    source.matches(concat!(".call(", "scope")).count()
        + source.matches(concat!(".call(&", "scope")).count()
        + source.matches(concat!(".call(&mut", "scope")).count()
}
