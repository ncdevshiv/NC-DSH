use url::Url;

use crate::script_vm::ScriptVm;

pub(super) fn register_import_map_source(
    vm: &mut ScriptVm,
    source: &str,
) -> std::result::Result<(), String> {
    vm.document_runtime.register_import_map_source(source)
}

pub(super) fn resolve_module_specifier(
    vm: &mut ScriptVm,
    specifier: &str,
    base_url: &Url,
) -> std::result::Result<Url, String> {
    vm.document_runtime
        .resolve_module_specifier(specifier, base_url)
}

pub(super) fn resolve_module_integrity(vm: &ScriptVm, url: &Url) -> Option<String> {
    vm.document_runtime.resolve_module_integrity(url)
}

pub(super) fn record_runtime_warning(vm: &mut ScriptVm, message: std::fmt::Arguments<'_>) {
    vm.record_runtime_warning(message);
}

pub(super) fn next_inline_module_eval_id(vm: &mut ScriptVm) -> u64 {
    vm.document_runtime.next_inline_module_eval_id()
}
