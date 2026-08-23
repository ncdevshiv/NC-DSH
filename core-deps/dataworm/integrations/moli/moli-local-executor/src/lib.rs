//! Local executor ownership tracking for JavaScript-affine work.
//!
//! The types here identify which local executor lane currently owns execution
//! and provide guarded access paths for code that must stay on the JS owner
//! thread. The crate does not own a JS runtime; it only models executor-lane
//! affinity and routing.

mod domain;
mod executor;
mod tls;

pub use domain::{
    JS_LOCAL_EXECUTOR_SCAFFOLD_LANE_ID, JsLocalExecutionDomain, JsLocalExecutorAccessContext,
    JsLocalExecutorDomainRelation, JsOwnerLocalRuntimeAccessPath, JsOwnerLocalRuntimeEntryPath,
};
pub use executor::JsLocalExecutor;
pub use tls::{
    current_js_local_execution_domain, current_js_local_executor_lane_id,
    is_on_current_thread_outside_js_local_lane, is_on_js_local_executor,
    is_on_named_owner_execution_lane_for, is_on_scoped_js_local_executor_lane,
    is_on_script_execution_lane_for, scope_on_scaffold_js_local_executor,
};

// Run renderer-owned JS work inside a dedicated single-thread execution lane.
//
// This helper exists because "task-driven" does *not* mean "run page JS on
// background Tokio worker threads".
//
// The browser-shaped constraint is stricter than that:
// - preload / fetch / completion notification may happen off the page owner chain
// - but actual JS execution, DOM runtime mutation, V8 isolate access, and
//   microtask checkpoints must stay on one page-owned execution lane
// - many of the values involved are better treated as `!Send`, even if the
//   type system does not yet force every boundary
//
// The render_runtime uses `tokio::runtime::LocalRuntime` (via `build_local()`)
// which natively supports `spawn_local` without `LocalSet`. V8 foreground tasks
// are routed to this runtime via the custom V8 Platform implementation.
//
// `JsLocalExecutor` provides lane tracking and ownership:
// - `tokio::spawn` remains appropriate for background work such as network
//   preload and fetch
// - `JsLocalExecutor::run(...)` is where renderer-owned JS work lives
// - later page/document task queues can schedule turns *onto* this local owner
//   lane without pretending that JS itself became multi-threaded
//
// This helper is intentionally narrow:
// - it does *not* change visible script timing by itself
// - it does *not* implement browser task queues by itself
// - it does *not* make JS execution multi-threaded
// - it only makes the owner lane explicit so later task-driven work is built
//   on a stable single-thread base instead of ordinary Tokio task interleaving

#[cfg(test)]
mod tests {
    use crate::{
        JS_LOCAL_EXECUTOR_SCAFFOLD_LANE_ID, JsLocalExecutionDomain, JsLocalExecutor,
        JsLocalExecutorAccessContext, JsLocalExecutorDomainRelation,
        current_js_local_execution_domain, current_js_local_executor_lane_id,
        is_on_scoped_js_local_executor_lane, scope_on_scaffold_js_local_executor,
    };

    #[test]
    fn current_domain_relation_distinguishes_matching_different_and_scaffold_lanes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime should build");
        let first_executor = JsLocalExecutor::new();
        let second_executor = JsLocalExecutor::new();

        runtime.block_on(async move {
            assert_eq!(
                first_executor.current_domain_relation(),
                JsLocalExecutorDomainRelation::Outside
            );
            scope_on_scaffold_js_local_executor(async {
                assert_eq!(
                    first_executor.current_domain_relation(),
                    JsLocalExecutorDomainRelation::ScaffoldLane
                );
            })
            .await;
            let first_executor_for_lane = first_executor.clone();
            let second_executor_for_lane = second_executor.clone();
            first_executor
                .run(async move {
                    assert_eq!(
                        first_executor_for_lane.current_domain_relation(),
                        JsLocalExecutorDomainRelation::MatchingNamedLane
                    );
                    assert_eq!(
                        second_executor_for_lane.current_domain_relation(),
                        JsLocalExecutorDomainRelation::DifferentNamedLane
                    );
                    let first_executor_for_scaffold = first_executor_for_lane.clone();
                    scope_on_scaffold_js_local_executor(async move {
                        assert_eq!(
                            first_executor_for_scaffold.current_domain_relation(),
                            JsLocalExecutorDomainRelation::MatchingNamedLane
                        );
                    })
                    .await;
                })
                .await;
        });
    }

    #[test]
    fn current_access_context_distinguishes_current_thread_outside_lane() {
        let current_thread_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime should build");
        let multi_thread_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("multi-thread runtime should build");
        let executor = JsLocalExecutor::new();

        current_thread_runtime.block_on(async {
            assert_eq!(
                executor.current_access_context(),
                JsLocalExecutorAccessContext::CurrentThreadOutsideLane
            );
        });

        multi_thread_runtime.block_on(async {
            assert_eq!(
                executor.current_access_context(),
                JsLocalExecutorAccessContext::Outside
            );
        });
    }

    #[test]
    fn nested_run_on_different_executor_enters_different_lane() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime should build");
        let first_executor = JsLocalExecutor::new();
        let second_executor = JsLocalExecutor::new();

        runtime.block_on(async move {
            first_executor
                .run(async move {
                    let first_lane = current_js_local_executor_lane_id()
                        .expect("first executor should bind a local lane");
                    second_executor
                        .run(async move {
                            let second_lane = current_js_local_executor_lane_id()
                                .expect("second executor should bind a local lane");
                            assert_ne!(first_lane, second_lane);
                        })
                        .await;
                    assert_eq!(current_js_local_executor_lane_id(), Some(first_lane));
                })
                .await;
        });
    }

    #[test]
    fn scope_binds_scaffold_lane_when_no_executor_lane_is_active() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime should build");

        runtime.block_on(async move {
            assert_eq!(current_js_local_executor_lane_id(), None);
            scope_on_scaffold_js_local_executor(async move {
                assert_eq!(
                    current_js_local_executor_lane_id(),
                    Some(JS_LOCAL_EXECUTOR_SCAFFOLD_LANE_ID)
                );
                assert_eq!(
                    current_js_local_execution_domain(),
                    JsLocalExecutionDomain::ScaffoldLane
                );
                assert!(is_on_scoped_js_local_executor_lane());
            })
            .await;
            assert_eq!(current_js_local_executor_lane_id(), None);
            assert_eq!(
                current_js_local_execution_domain(),
                JsLocalExecutionDomain::Outside
            );
        });
    }

    #[test]
    fn scope_preserves_active_executor_lane() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime should build");
        let executor = JsLocalExecutor::new();

        runtime.block_on(async move {
            executor
                .run(async move {
                    let active_lane = current_js_local_executor_lane_id()
                        .expect("executor run should bind a lane");
                    assert_eq!(
                        current_js_local_execution_domain(),
                        JsLocalExecutionDomain::NamedLane(active_lane)
                    );
                    assert!(!is_on_scoped_js_local_executor_lane());
                    scope_on_scaffold_js_local_executor(async move {
                        assert_eq!(current_js_local_executor_lane_id(), Some(active_lane));
                        assert_eq!(
                            current_js_local_execution_domain(),
                            JsLocalExecutionDomain::NamedLane(active_lane)
                        );
                        assert!(!is_on_scoped_js_local_executor_lane());
                    })
                    .await;
                    assert_eq!(current_js_local_executor_lane_id(), Some(active_lane));
                })
                .await;
        });
    }
}
