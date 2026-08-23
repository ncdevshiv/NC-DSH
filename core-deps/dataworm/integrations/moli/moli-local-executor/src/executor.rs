use std::{future::Future, sync::Arc};

use tokio::sync::Mutex;

use crate::domain::{
    JsLocalExecutionDomain, JsLocalExecutorAccessContext, JsLocalExecutorDomainRelation,
    JsOwnerLocalRuntimeAccessPath, JsOwnerLocalRuntimeEntryPath,
};
use crate::tls::{
    JS_LOCAL_EXECUTOR_MARKER, NEXT_JS_LOCAL_EXECUTOR_LANE_ID, current_js_local_execution_domain,
    current_js_local_executor_lane_id, is_on_current_thread_outside_js_local_lane,
};

use std::sync::atomic::Ordering;

#[derive(Debug)]
struct JsLocalExecutorState {
    owner_lane: Mutex<()>,
    lane_id: usize,
}

#[derive(Clone, Debug)]
pub struct JsLocalExecutor {
    state: Arc<JsLocalExecutorState>,
}

impl JsLocalExecutor {
    pub fn new() -> Self {
        Self {
            state: Arc::new(JsLocalExecutorState {
                owner_lane: Mutex::new(()),
                lane_id: NEXT_JS_LOCAL_EXECUTOR_LANE_ID.fetch_add(1, Ordering::Relaxed),
            }),
        }
    }
    pub async fn run<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        if current_js_local_executor_lane_id() == Some(self.state.lane_id) {
            return future.await;
        }

        let _owner_lane = self.state.owner_lane.lock().await;
        JS_LOCAL_EXECUTOR_MARKER
            .scope(self.state.lane_id, future)
            .await
    }

    pub async fn scope_on_current_thread<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        // This only marks the current task as executing in this lane; it does
        // not acquire `owner_lane`. Callers must already own the lane through
        // `run` or be performing single-threaded bootstrap work that cannot
        // race with another `run` entry.
        JS_LOCAL_EXECUTOR_MARKER
            .scope(self.state.lane_id, future)
            .await
    }
    pub fn current_domain_relation(&self) -> JsLocalExecutorDomainRelation {
        match current_js_local_execution_domain() {
            JsLocalExecutionDomain::NamedLane(active_lane) if active_lane == self.state.lane_id => {
                JsLocalExecutorDomainRelation::MatchingNamedLane
            }
            JsLocalExecutionDomain::NamedLane(_) => {
                JsLocalExecutorDomainRelation::DifferentNamedLane
            }
            JsLocalExecutionDomain::ScaffoldLane => JsLocalExecutorDomainRelation::ScaffoldLane,
            JsLocalExecutionDomain::Outside => JsLocalExecutorDomainRelation::Outside,
        }
    }

    pub fn current_access_context(&self) -> JsLocalExecutorAccessContext {
        match self.current_domain_relation() {
            JsLocalExecutorDomainRelation::MatchingNamedLane => {
                JsLocalExecutorAccessContext::MatchingNamedLane
            }
            JsLocalExecutorDomainRelation::DifferentNamedLane => {
                JsLocalExecutorAccessContext::DifferentNamedLane
            }
            JsLocalExecutorDomainRelation::ScaffoldLane => {
                JsLocalExecutorAccessContext::ScaffoldLane
            }
            JsLocalExecutorDomainRelation::Outside
                if is_on_current_thread_outside_js_local_lane() =>
            {
                JsLocalExecutorAccessContext::CurrentThreadOutsideLane
            }
            JsLocalExecutorDomainRelation::Outside => JsLocalExecutorAccessContext::Outside,
        }
    }
    pub fn current_owner_local_runtime_access_path(&self) -> JsOwnerLocalRuntimeAccessPath {
        match self.current_access_context() {
            JsLocalExecutorAccessContext::MatchingNamedLane => {
                JsOwnerLocalRuntimeAccessPath::DirectNamedLane
            }
            JsLocalExecutorAccessContext::CurrentThreadOutsideLane => {
                JsOwnerLocalRuntimeAccessPath::CurrentThreadFallback
            }
            JsLocalExecutorAccessContext::DifferentNamedLane
            | JsLocalExecutorAccessContext::ScaffoldLane
            | JsLocalExecutorAccessContext::Outside => JsOwnerLocalRuntimeAccessPath::ExecutorHop,
        }
    }
    pub fn current_owner_local_runtime_entry_path(&self) -> JsOwnerLocalRuntimeEntryPath {
        match self.current_owner_local_runtime_access_path() {
            JsOwnerLocalRuntimeAccessPath::DirectNamedLane => {
                JsOwnerLocalRuntimeEntryPath::DirectNamedLane
            }
            JsOwnerLocalRuntimeAccessPath::CurrentThreadFallback
            | JsOwnerLocalRuntimeAccessPath::ExecutorHop => {
                JsOwnerLocalRuntimeEntryPath::ExecutorHop
            }
        }
    }
}

impl Default for JsLocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}
