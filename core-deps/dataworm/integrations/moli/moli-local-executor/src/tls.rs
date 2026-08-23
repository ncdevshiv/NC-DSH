use std::{future::Future, sync::atomic::AtomicUsize};

use crate::domain::{
    JS_LOCAL_EXECUTOR_SCAFFOLD_LANE_ID, JsLocalExecutionDomain, JsLocalExecutorAccessContext,
};
use crate::executor::JsLocalExecutor;

tokio::task_local! {
    pub(super) static JS_LOCAL_EXECUTOR_MARKER: usize;
}

pub(super) static NEXT_JS_LOCAL_EXECUTOR_LANE_ID: AtomicUsize = AtomicUsize::new(1);

pub fn is_on_js_local_executor() -> bool {
    !matches!(
        current_js_local_execution_domain(),
        JsLocalExecutionDomain::Outside
    )
}
pub fn is_on_scoped_js_local_executor_lane() -> bool {
    matches!(
        current_js_local_execution_domain(),
        JsLocalExecutionDomain::ScaffoldLane
    )
}

pub fn is_on_current_thread_outside_js_local_lane() -> bool {
    matches!(
        current_js_local_execution_domain(),
        JsLocalExecutionDomain::Outside
    ) && is_on_current_thread_runtime()
}

pub fn is_on_named_owner_execution_lane_for(executor: &JsLocalExecutor) -> bool {
    matches!(
        executor.current_access_context(),
        JsLocalExecutorAccessContext::MatchingNamedLane
    )
}
pub fn is_on_script_execution_lane_for(executor: &JsLocalExecutor) -> bool {
    matches!(
        executor.current_access_context(),
        JsLocalExecutorAccessContext::MatchingNamedLane
            | JsLocalExecutorAccessContext::ScaffoldLane
    )
}

pub fn current_js_local_execution_domain() -> JsLocalExecutionDomain {
    match current_js_local_executor_lane_id() {
        Some(JS_LOCAL_EXECUTOR_SCAFFOLD_LANE_ID) => JsLocalExecutionDomain::ScaffoldLane,
        Some(lane_id) => JsLocalExecutionDomain::NamedLane(lane_id),
        None => JsLocalExecutionDomain::Outside,
    }
}

pub fn current_js_local_executor_lane_id() -> Option<usize> {
    JS_LOCAL_EXECUTOR_MARKER.try_with(|lane_id| *lane_id).ok()
}

fn is_on_current_thread_runtime() -> bool {
    tokio::runtime::Handle::try_current()
        .map(|handle| handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread)
        .unwrap_or(false)
}
pub async fn scope_on_scaffold_js_local_executor<F>(future: F) -> F::Output
where
    F: Future,
{
    let lane_id = current_js_local_executor_lane_id().unwrap_or(JS_LOCAL_EXECUTOR_SCAFFOLD_LANE_ID);
    JS_LOCAL_EXECUTOR_MARKER.scope(lane_id, future).await
}
