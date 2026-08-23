This vendored `rusty_v8` tree is intentionally slimmed for Moli's
default prebuilt workflow.

Base:
- rusty_v8 crate version `146.9.0`
- V8 version `14.6.202.24`
- the Rust wrapper and public headers are vendored here, while the default
  build links the matching upstream prebuilt archive

Kept:
- `build.rs`
- `Cargo.toml`
- `README.md`
- `src/`
- `gen/`
- `v8/include/`

Moli extensions:
- `build.rs` compiles Moli's small V8 C++ shims through
  `build_moli_v8_ext()`. Keep new Moli-owned C++ shims in separate
  `src/*_ext.cc` files instead of editing upstream `src/binding.cc` directly.
- `src/object_template_ext.cc` exposes embedder APIs needed by Moli DOM
  bindings.
- `src/function_template_ext.cc` exposes native FunctionTemplate instance
  checks used by WebIDL interface conversions.
- `src/inspector_context_ext.cc` exposes
  `v8_inspector::V8ContextInfo::executionContextId(context)` so CDP isolated
  worlds can store the same inspector execution context id Chromium returns from
  `Page.createIsolatedWorld`, without relying on `Runtime.executionContextCreated`
  events.
- `src/module_ext.cc` exposes the synthetic module export hook Moli needs
  to write V8's uninitialized binding sentinel for wasm `v128` namespace cells.
- `src/wasm_ext.cc` exposes wasm compile-with-options support for JS string
  builtins and imported string constants while preserving the prebuilt rusty_v8
  workflow.

Tracked upstream backports:
- `src/isolate.rs` carries Moli's version-specific backport of
  [rusty_v8 #1978](https://github.com/denoland/rusty_v8/pull/1978), committed
  locally as `3b6484838`.
- The backport keeps `ANNEX_SLOT` and the isolate-owned `Arc<IsolateAnnex>`
  alive through V8's final `Isolate::Dispose`, drains guaranteed finalizers
  outside annex borrows, and drains again before releasing the annex.
- This is not a direct cherry-pick of upstream
  `e5abf2b25ab2faf784ee40dd1b25f15c489f8c0b`. Upstream's newer implementation
  owns the annex with `Box`, has a separate handle/liveness model, and marks
  the handle disposed before its pre-dispose finalizer drain.
- The 146.9.0 backport intentionally runs the first finalizer drain while the
  old `IsolateHandle` can still reset V8 weak global nodes. This is required
  for `ContextAnnex.self_weak`; otherwise its `WeakData` can be released before
  V8 has stopped using the callback parameter.
- Ordinary isolate teardown and snapshot-creator teardown are covered by
  `context_annex_weak_handles_are_safe_during_isolate_teardown` and
  `snapshot_creator_cleans_up_context_annex_before_creating_blob` in
  `moli-renderer-v8/src/script_vm/document_isolate.rs`.
- Full diagnosis, upstream comparison, and existing verification evidence are
  recorded in
  `../../docs/rusty-v8-isolate-annex-teardown-report-2026-07-26.md`.

Removed:
- GN/Ninja/Chromium source-build inputs such as `build/`, `buildtools/`,
  `third_party/`, `tools/`, the rest of `v8/`, and crate-local tests/examples.

Implications:
- default prebuilt linking remains supported
- `V8_FROM_SOURCE=1` is only supported after restoring the full upstream
  source-build layout; the current slim tree fails early with the missing
  source-build inputs listed

If you need source builds again, restore the full upstream `rusty_v8` crate
layout first. Keep the early layout check in `build.rs`; it prevents the slim
prebuilt tree from pretending it can build or patch V8 internals.

When upgrading rusty_v8 to `149.2.0` or newer:
- keep the two Moli teardown regressions as upgrade gates
- compare the new annex and weak-handle ownership model before removing the
  local backport
- remove the 146.9.0 backport once the upstream implementation passes those
  gates; do not stack both teardown implementations
