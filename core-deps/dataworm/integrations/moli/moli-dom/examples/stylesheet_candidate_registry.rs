use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use moli_dom::native::{DomHost, NativeDom};
use url::Url;

const CANDIDATES: usize = 1_000;
const ORDINARY_NODES_PER_CANDIDATE: usize = 9;
const ORDINARY_SUBTREE_NODES: usize = CANDIDATES * (ORDINARY_NODES_PER_CANDIDATE + 1);

struct CountingAllocator;

static ALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);
static REALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        DEALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
        DEALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            REALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            DEALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        new_pointer
    }
}

#[derive(Clone, Copy)]
struct AllocationSnapshot {
    allocation_calls: u64,
    reallocation_calls: u64,
    deallocation_calls: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
}

impl AllocationSnapshot {
    fn capture() -> Self {
        Self {
            allocation_calls: ALLOCATION_CALLS.load(Ordering::Relaxed),
            reallocation_calls: REALLOCATION_CALLS.load(Ordering::Relaxed),
            deallocation_calls: DEALLOCATION_CALLS.load(Ordering::Relaxed),
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
            deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        }
    }

    fn since(self, before: Self) -> Self {
        Self {
            allocation_calls: self.allocation_calls - before.allocation_calls,
            reallocation_calls: self.reallocation_calls - before.reallocation_calls,
            deallocation_calls: self.deallocation_calls - before.deallocation_calls,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            deallocated_bytes: self.deallocated_bytes - before.deallocated_bytes,
        }
    }
}

struct Measurement {
    elapsed: Duration,
    allocations: AllocationSnapshot,
    stylesheet_owner_changes: usize,
}

fn measure_mutation(
    mutation: impl FnOnce() -> moli_dom::native::DomMutationEffects,
) -> Measurement {
    measure_operation(|| mutation().stylesheet_owners().changes().len())
}

fn measure_operation(operation: impl FnOnce() -> usize) -> Measurement {
    let allocations_before = AllocationSnapshot::capture();
    let started = Instant::now();
    let stylesheet_owner_changes = operation();
    let elapsed = started.elapsed();
    let allocations = AllocationSnapshot::capture().since(allocations_before);
    Measurement {
        elapsed,
        allocations,
        stylesheet_owner_changes,
    }
}

fn main() {
    let mut host = DomHost::from_dom(NativeDom::new_html(
        Url::parse("https://stylesheet-candidate-bench.test/").expect("valid benchmark URL"),
    ));
    let document = host.document_handle();
    let before = host.create_element("style");
    let after = host.create_element("link");
    assert!(host.append_child(document, before));
    assert!(host.append_child(document, after));

    let candidate_wrapper = host.create_element("section");
    let mut candidate_handles = Vec::with_capacity(CANDIDATES);
    for _ in 0..CANDIDATES {
        for _ in 0..ORDINARY_NODES_PER_CANDIDATE {
            let ordinary = host.create_element("div");
            assert!(host.append_child_without_mutation_effects(candidate_wrapper, ordinary));
        }
        let style = host.create_element("style");
        assert!(host.append_child_without_mutation_effects(candidate_wrapper, style));
        candidate_handles.push(style);
    }

    let ordinary_wrapper = host.create_element("section");
    for _ in 0..ORDINARY_SUBTREE_NODES {
        let ordinary = host.create_element("div");
        assert!(host.append_child_without_mutation_effects(ordinary_wrapper, ordinary));
    }

    let candidate_fragment = host.create_document_fragment();
    let mut fragment_candidate_handles = Vec::with_capacity(CANDIDATES);
    for _ in 0..CANDIDATES {
        for _ in 0..ORDINARY_NODES_PER_CANDIDATE {
            let ordinary = host.create_element("div");
            assert!(host.append_child_without_mutation_effects(candidate_fragment, ordinary));
        }
        let style = host.create_element("style");
        assert!(host.append_child_without_mutation_effects(candidate_fragment, style));
        fragment_candidate_handles.push(style);
    }

    let candidate_insert =
        measure_mutation(|| host.insert_before_effects(document, candidate_wrapper, Some(after)));
    let registered = host.stylesheet_candidate_handles_for_tree_scope(document);
    assert_eq!(registered.len(), CANDIDATES + 2);
    assert_eq!(registered.first(), Some(&before));
    assert_eq!(registered.last(), Some(&after));
    assert_eq!(&registered[1..registered.len() - 1], candidate_handles);
    assert_eq!(candidate_insert.stylesheet_owner_changes, CANDIDATES);

    let candidate_remove =
        measure_mutation(|| host.remove_child_effects(document, candidate_wrapper));
    assert_eq!(
        host.stylesheet_candidate_handles_for_tree_scope(document),
        vec![before, after].into()
    );
    assert_eq!(candidate_remove.stylesheet_owner_changes, CANDIDATES);

    let sequential_wrapper = host.create_element("section");
    assert!(host.insert_before(document, sequential_wrapper, Some(after)));
    let sequential_candidate_handles = (0..CANDIDATES)
        .map(|_| host.create_element("style"))
        .collect::<Vec<_>>();
    let sequential_candidate_append = measure_operation(|| {
        sequential_candidate_handles
            .iter()
            .map(|&style| {
                host.append_child_effects(sequential_wrapper, style)
                    .stylesheet_owners()
                    .changes()
                    .len()
            })
            .sum()
    });
    let registered = host.stylesheet_candidate_handles_for_tree_scope(document);
    assert_eq!(registered.len(), CANDIDATES + 2);
    assert_eq!(registered.first(), Some(&before));
    assert_eq!(registered.last(), Some(&after));
    assert_eq!(
        &registered[1..registered.len() - 1],
        sequential_candidate_handles
    );
    assert_eq!(
        sequential_candidate_append.stylesheet_owner_changes,
        CANDIDATES
    );
    assert_eq!(
        host.remove_child_effects(document, sequential_wrapper)
            .stylesheet_owners()
            .changes()
            .len(),
        CANDIDATES
    );

    let fragment_insert =
        measure_mutation(|| host.insert_before_effects(document, candidate_fragment, Some(after)));
    let registered = host.stylesheet_candidate_handles_for_tree_scope(document);
    assert_eq!(registered.len(), CANDIDATES + 2);
    assert_eq!(registered.first(), Some(&before));
    assert_eq!(registered.last(), Some(&after));
    assert_eq!(
        &registered[1..registered.len() - 1],
        fragment_candidate_handles
    );
    assert_eq!(fragment_insert.stylesheet_owner_changes, CANDIDATES);

    let registered_before_ordinary = registered;
    let ordinary_insert =
        measure_mutation(|| host.insert_before_effects(document, ordinary_wrapper, Some(after)));
    assert_eq!(ordinary_insert.stylesheet_owner_changes, 0);
    assert_eq!(
        host.stylesheet_candidate_handles_for_tree_scope(document),
        registered_before_ordinary
    );

    let shadow_host = host.create_element("section");
    let shadow_root = host
        .attach_shadow_root(shadow_host, "open")
        .expect("benchmark host accepts a shadow root");
    let shadow_style = host.create_element("style");
    assert!(host.append_child(shadow_root, shadow_style));
    assert!(host.append_child(document, shadow_host));
    let registry_snapshot = host.snapshot_document();
    let snapshot_document_candidates =
        registry_snapshot.stylesheet_candidate_handles_for_tree_scope(document);
    let snapshot_shadow_candidates =
        registry_snapshot.stylesheet_candidate_handles_for_tree_scope(shadow_root);
    let candidate_reads = measure_operation(|| {
        for _ in 0..100_000 {
            let candidates = host.stylesheet_candidate_handles_for_tree_scope(document);
            assert!(Arc::ptr_eq(&candidates, &snapshot_document_candidates));
        }
        0
    });
    let snapshot_live_style = host.create_element("style");
    let snapshot_live_scope_mutation =
        measure_mutation(|| host.append_child_effects(document, snapshot_live_style));
    let live_document_candidates = host.stylesheet_candidate_handles_for_tree_scope(document);
    let live_shadow_candidates = host.stylesheet_candidate_handles_for_tree_scope(shadow_root);
    assert!(!Arc::ptr_eq(
        &live_document_candidates,
        &snapshot_document_candidates
    ));
    assert!(Arc::ptr_eq(
        &live_shadow_candidates,
        &snapshot_shadow_candidates
    ));

    print_measurement("candidate_insert", candidate_insert);
    print_measurement("candidate_remove", candidate_remove);
    print_measurement("sequential_candidate_append", sequential_candidate_append);
    print_measurement("fragment_insert", fragment_insert);
    print_measurement("ordinary_insert", ordinary_insert);
    print_measurement("candidate_read_arc_clone_x100000", candidate_reads);
    print_measurement(
        "snapshot_live_single_scope_mutation",
        snapshot_live_scope_mutation,
    );
    println!(
        "candidate_nodes={} candidates={} ordinary_nodes={}",
        ORDINARY_SUBTREE_NODES, CANDIDATES, ORDINARY_SUBTREE_NODES
    );
}

fn print_measurement(label: &str, measurement: Measurement) {
    println!(
        "{label} elapsed_ms={} allocation_calls={} reallocation_calls={} deallocation_calls={} allocated_bytes={} deallocated_bytes={} stylesheet_owner_changes={}",
        measurement.elapsed.as_millis(),
        measurement.allocations.allocation_calls,
        measurement.allocations.reallocation_calls,
        measurement.allocations.deallocation_calls,
        measurement.allocations.allocated_bytes,
        measurement.allocations.deallocated_bytes,
        measurement.stylesheet_owner_changes,
    );
}
