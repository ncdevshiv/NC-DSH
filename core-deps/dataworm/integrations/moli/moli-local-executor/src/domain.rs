pub const JS_LOCAL_EXECUTOR_SCAFFOLD_LANE_ID: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsLocalExecutionDomain {
    NamedLane(usize),
    ScaffoldLane,
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsLocalExecutorDomainRelation {
    MatchingNamedLane,
    DifferentNamedLane,
    ScaffoldLane,
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsLocalExecutorAccessContext {
    MatchingNamedLane,
    DifferentNamedLane,
    ScaffoldLane,
    CurrentThreadOutsideLane,
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsOwnerLocalRuntimeAccessPath {
    DirectNamedLane,
    CurrentThreadFallback,
    ExecutorHop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsOwnerLocalRuntimeEntryPath {
    DirectNamedLane,
    ExecutorHop,
}
