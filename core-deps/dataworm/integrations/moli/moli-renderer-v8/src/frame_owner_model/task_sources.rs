/// Test-only observation of one child-frame semantic executor turn.
///
/// Every variant is backed by the one stable production ChildFrameTask
/// family. Keeping this type test-only avoids leaking test observations into
/// the scheduler taxonomy.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildFrameSemanticTurnKind {
    RealmMaterialization,
    NavigationCommit,
    DocumentLifecycle,
    DocumentScriptReady,
    HostLoad,
    ClassicScriptSourceLoad,
    ParserModuleRootStart,
}
