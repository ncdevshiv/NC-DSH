pub(crate) use moli_local_executor::{JsLocalExecutor, is_on_named_owner_execution_lane_for};

#[cfg(test)]
pub(crate) use moli_local_executor::{
    JsLocalExecutorAccessContext, JsOwnerLocalRuntimeAccessPath, JsOwnerLocalRuntimeEntryPath,
    is_on_scoped_js_local_executor_lane, is_on_script_execution_lane_for,
    scope_on_scaffold_js_local_executor,
};

pub use moli_local_executor::is_on_js_local_executor;
