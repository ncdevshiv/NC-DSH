use moli_module_script_tree as module_tree;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_MODULE_TREE_ID: AtomicU64 = AtomicU64::new(1);

fn next_module_tree_id() -> module_tree::ModuleTreeId {
    module_tree::ModuleTreeId(NEXT_MODULE_TREE_ID.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn parser_owned_tree_job(
    root: module_tree::ModuleRootInput,
) -> module_tree::ModuleScriptTreeJob {
    module_tree::ModuleScriptTreeJob::new(
        root,
        module_tree::ModuleTreeConfig {
            tree_id: next_module_tree_id(),
            owner: module_tree::ModuleTreeOwner::parser_pending_script(),
            ..module_tree::ModuleTreeConfig::default()
        },
    )
}

pub(super) fn runtime_module_script_tree_job(
    root: module_tree::ModuleRootInput,
) -> module_tree::ModuleScriptTreeJob {
    module_tree::ModuleScriptTreeJob::new(
        root,
        module_tree::ModuleTreeConfig {
            tree_id: next_module_tree_id(),
            owner: module_tree::ModuleTreeOwner::runtime_module_script(),
            ..module_tree::ModuleTreeConfig::default()
        },
    )
}

pub(super) fn dynamic_import_tree_job(
    root: module_tree::ModuleRootInput,
) -> module_tree::ModuleScriptTreeJob {
    module_tree::ModuleScriptTreeJob::new(
        root,
        module_tree::ModuleTreeConfig {
            tree_id: next_module_tree_id(),
            owner: module_tree::ModuleTreeOwner::dynamic_import(),
            ..module_tree::ModuleTreeConfig::default()
        },
    )
}
