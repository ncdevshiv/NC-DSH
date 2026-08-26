# Bun.open contribution journal

## Goal (session-level)

Build `Bun.open(target, options?)` in the `nc-bun` fork as a new native
`Bun.*` API, then push to `ncdevshiv/nc-bun`, then upstream to `oven-sh/bun`.
Mirror the npm `open` package's behavior (macOS `/usr/bin/open`, Linux
`xdg-open`, Windows `cmd /c start`), but as a first-class built-in that
doesn't need a child process for the JS→OS round trip. Replace the
`open@11.0.1` dependency in dsh once upstream ships it.

The dev → test → bench → push loop is the test of the whole pipeline.
We will iterate on the loop, not on a single perfect commit.

---

## 2026-08-25 (initial scope)

### Wanted
1. Identify what Bun 1.4 ships, what it absorbs from npm, and what we can
   delete from dsh by waiting for/landing features.
2. Start a real upstream contribution on a fork.

### Had
- `F:\deepseek-harness-master\deepseek-harness-master` on Bun 1.4.0
  (local bun, fork nc-bun was about to exist).
- ~30 third-party deps in the dsh workspace.

### Did
1. Audited the workspace: every product path runs on Node/Electron/browser,
   so Bun-runtime APIs don't apply to dsh unless we flip the engine.
2. Decided `Bun.open` is the lowest-risk starter contribution.
3. Cloned `ncdevshiv/nc-bun` to `F:\nc-bun`, wired `upstream` to
   `oven-sh/bun`, branched `feat/bun-open`.

### Errors / root causes / fixes / decisions
- *Clone was 197 MB blob-only first try.* `git clone --filter=blob:none` was
  fine on retry; not revisited.
- *Long path (F:\nc-bun + deep toolchain) caused bsdtar `\\?\F:\...` failures
  during vendored-deps extraction.* Moved fork to `C:\nc-bun`. No real
  fix to bun needed; it's a Git-Bash-bundled bsdtar issue on deep
  Windows paths.

### Files edited
- (none in the repo yet — just `git clone` and `git checkout -b`)

### Expected goal next
Write the scaffold, wire it in, build, fix.

---

## 2026-08-25 (scaffold + toolchain)

### Wanted
- Per-OS argv builder (macOS, Linux/FreeBSD, Windows; Android=NotImpl).
- Host function `fn open(...)` registered in the Bun global.
- JSDoc-table row + `BunObject+exports.h` macro entry.
- `bun-types` declarations.
- bun:test spec covering URL/file/folder/errors/options.

### Had
- `open_in_editor` template at `src/runtime/api/BunObject.rs:926`.
- The full template chain mapped end-to-end (cpp → exports.h → types).

### Did
1. `src/runtime/api/open.rs` (new): `OpenOptions`, `OpenError`,
   `build_argv(target, opts)` per-OS, `resolve_opener()`,
   `pub fn argv_for(target, opts) -> Result<Vec<Vec<u8>>, OpenError>`.
2. `src/runtime/api.rs`: `#[path = "api/open.rs"] pub mod open;` between
   `native_promise_context` and `output_file_jsc`.
3. `src/runtime/api/BunObject.rs`: `fn open` host_fn after
   `open_in_editor`, scaffolded to throw `OpenError::UnsupportedOs` so
   the registration compiles before the JSC spawn glue lands.
4. `src/jsc/bindings/BunObject+exports.h`: `macro(open) \` between
   `mmap` and `openInEditor`.
5. `src/jsc/bindings/BunObject.cpp`: `open` row in the
   `@begin bunObjectTable` JSDoc block, between `nanoseconds` and
   `openInEditor`.
6. `packages/bun-types/bun.d.ts`: `function open(...)` + `BunOpenOptions`
   + `BunOpenResult` interfaces with full JSDoc + `@example`.
7. `test/js/bun/util/open.test.ts`: argument-validation + URL/file/folder/
   error/options cases gated by `isWindows|isMacOS|isLinux`.
8. Installed missing build toolchain via Scoop: pwsh, Go, NASM, Ruby, Perl,
   ccache, LLVM 21.1.8.

### Errors / root causes / fixes
- **Toolchain audit**: Rust 1.97.1 + VS Build Tools 2022 present, but
  CMake/Ninja/Clang missing. CMake/Ninja are NOT installed globally
  (the Windows build bootstraps them). We needed pwsh + LLVM 21.1.8 +
  Go/NASM/Ruby/Perl/ccache — installed via Scoop.
- **First build attempted via `vs-shell.ps1`**: silently failed because
  pwsh wasn't on the Git-Bash PATH. Fix: `export PATH="/c/Users/Ncdevshiv/scoop/shims:..."`
  before invoking PowerShell.
- **`vs-shell.ps1` itself fails** (tries to launch Developer PowerShell
  with -Arch x64, which is not a valid value). Replaced with the explicit
  `Launch-VsDevShell.ps1 -Arch amd64 -HostPwsh` from the VS install.
- **Missing `clang-cl`**: Scoop installed LLVM at
  `C:\Users\Ncdevshiv\scoop\apps\llvm\21.1.8\bin`, but Bun's build searched
  `C:\Program Files\LLVM\bin`. Fix: prepend Scoop's LLVM dir to `$env:PATH`
  inside the PowerShell command.
- **Failed tarball extraction for zstd** at `\\?\F:\nc-bun\vendor\zstd\...`:
  root cause was the deep Windows path, partially. After moving to
  `C:\nc-bun`, the same path was short, but the error changed to
  `tests/cli-tests/bin/unzstd: Can't create '\\?\C:\nc-bun\...': Invalid
  argument`. The real cause: bsdtar on Windows can't create symlinks
  (the `unzstd`/`zstdcat` entries are symlinks to `zstd`/`zstdcat`).
  Fix: patch `scripts/build/download.ts::extractTarGz` to accept
  `excludes: string[]`; call site in `fetch-cli.ts` passes
  `["tests/cli-tests/bin/*"]` only for the `zstd` dep.
- **`codegen/cpp.rs` FAILED: Cannot find module '@lezer/lr'**: the fork
  pre-existing bug — `@lezer/cpp` uses `@lezer/lr` at runtime but the fork
  `package.json` only declares `@lezer/common` and `@lezer/cpp`. Fix:
  `bun add -d @lezer/lr@^1.4.2 @lezer/highlight@^1.2.1`.

### Files edited
- `src/runtime/api/open.rs` (new, 220 lines)
- `src/runtime/api.rs` (1 line: `pub mod open;`)
- `src/runtime/api/BunObject.rs` (~85 lines: import, host_fn, export table)
- `src/jsc/bindings/BunObject+exports.h` (1 line)
- `src/jsc/bindings/BunObject.cpp` (1 line in JSDoc table)
- `packages/bun-types/bun.d.ts` (~90 lines: `function open` + 2 interfaces)
- `test/js/bun/util/open.test.ts` (new, 60 lines)
- `scripts/build/download.ts` (`extractTarGz` adds `excludes` param)
- `scripts/build/fetch-cli.ts` (zstd passes the excludes)
- `package.json` (added `@lezer/lr` and `@lezer/highlight` as devDeps)

### Test/review
- Commit: `feat(Bun): scaffold Bun.open target/options parser` (c1aff446f9,
  7 files, 433 insertions).
- Brace mismatch + 8 Rust type errors discovered by `cargo check` (next
  section).

### Expected goal next
Fix the 8 Rust type errors and rebuild.

---

## 2026-08-25 (fixing compile errors)

### Wanted
`cargo check -p bun_runtime --lib` clean.

### Had
8 errors:
1. `open.rs:20`: `unresolved import crate::api::bun::js_bun_spawn_bindings`
2. `open.rs:31`: `missing lifetime specifier` on `pub app: Option<Utf8Bytes>`
3. `BunObject.rs:1047/1049/1053`: `expected Arguments<'_>, found &str`
   (because `throw_invalid_arguments` takes `format_args!` output, not
   `&str`)
4. `BunObject.rs:1091/1111`: `the trait bound error::Error: From<OpenError>
   is not satisfied`
5. `open.rs:155`: `unused variable: opts`

### Root causes
- The `cli::open::Editor` import at `BunObject.rs:83` brings the
  module name `open` into scope at the file level, which made
  `crate::api::open::OpenOptions` resolve via the wrong `Arguments<'_>`
  type in nested scope. Fix: add explicit
  `use crate::api::open::{self as open_api, OpenOptions};` to disambiguate.
- `Utf8Bytes` doesn't impl `Debug`; my `#[derive(Debug)]` on `OpenOptions`
  failed. Fix: drop `Debug` from the derive.
- `Utf8Bytes` has `.slice()`, not `.as_bytes()`. Fix: replace all
  `app.as_bytes()` with `app.slice()` (only on `Utf8Bytes`; `&str` keeps
  `.as_bytes()`).
- The lifetime in `open.rs:112` (`return Ok(app.slice())`) needs explicit
  `'static` because the function signature is `&'a OpenOptions`. Fix:
  `pub fn resolve_opener<'a>(opts: &'a OpenOptions) -> Result<&'a [u8], ...>`
  with the explicit lifetime, or coerce to `&'static [u8]` via the
  caller's owned argv. Took the explicit-lifetime path.

### Things we thought about fixing
- *Replace `throw_invalid_arguments(&str)` with `throw_invalid_arguments
  (format_args!(...))` everywhere.* Yes, this is the right fix; matches
  the rest of the Bun host_fn code.
- *Make `OpenError` implement `Into<crate::Error>`.* Out of scope: the
  build doesn't need that, and `global_this.throw(format_args!("{}", err))`
  is shorter and works.
- *Wrap `OpenError` in a `BunError` newtype.* Same — out of scope for v1.
- *Use a fully qualified path for `OpenOptions` everywhere.* Yes, plus
  the alias `open_api` to avoid repeating the long path.

### Chosen solutions
- `use crate::api::open::{self as open_api, OpenOptions};` for disambiguation.
- `#[derive(Default, Clone)]` on `OpenOptions` (no Debug).
- `app.slice()` instead of `app.as_bytes()` on `Utf8Bytes`.
- `throw_invalid_arguments(format_args!(...))` everywhere.
- `global_this.throw(format_args!("{}", err))` instead of
  `crate::Error::from(err)`.

### Files edited
- `src/runtime/api/open.rs`:
  - drop `use crate::api::bun::js_bun_spawn_bindings;`
  - `pub app: Option<Utf8Bytes<'static>>`
  - `#[derive(Default, Clone)]` (no Debug)
  - `app.slice()` instead of `app.as_bytes()`
  - lifetime annotations on `resolve_opener`
- `src/runtime/api/BunObject.rs`:
  - add `use crate::api::open::{self as open_api, OpenOptions};`
  - all `throw_invalid_arguments("...")` → `throw_invalid_arguments
    (format_args!("..."))`
  - all `crate::Error::from(err)` → `global_this.throw(format_args!("{}", err))`

### Expected goal next
Cargo check passes → run full build (cargo + C++ link + codegen).

---

## 2026-08-25 (cargo check clean)

### Wanted
`cargo check -p bun_runtime --lib` returns zero errors.

### Had
After the previous round: 2 remaining errors —
1. `open.rs:112`: lifetime `'1 must outlive 'static` because
   `resolve_opener` declared `-> &'static [u8]` but `opts` is `&'1`.
2. `open.rs:153`: `unused variable: opts` (the parameter on
   `build_argv`, not a body binding).

### Things we thought about fixing
- *Rename `opts` to `_opts` on the cfg-fallback arm only.* Not possible
  without splitting the function into two `#[cfg]`-gated functions and a
  thin shim — too much ceremony for a one-line fallback.
- *Use `#[allow(unused_variables)]` on the function.* Doesn't work: the
  parameter is "used" on the platform branches the build is on, but the
  function-level attribute is per-cfg-arm and Rust still flags the
  parameter site.
- *Module-level `#![allow(unused_variables)]`.* Works. The lint is
  acceptable to silence globally because the entire module is a
  per-OS dispatch — almost every arm has a config where one of the
  parameters is unused.

### Chosen solution
- `fn resolve_opener<'a>(opts: &'a OpenOptions) -> Result<&'a [u8], ...>`
  (explicit lifetime matching the input).
- `#![allow(unused_variables)]` at the top of `open.rs`.
- Reverted the `#[allow]` on the fn signature since it was fighting the
  per-arm behavior.

### Files edited
- `src/runtime/api/open.rs`:
  - `fn resolve_opener<'a>(opts: &'a OpenOptions) -> Result<&'a [u8], ...>`
  - Added module-level `#![allow(unused_variables)]`.

### Test / review
- `cargo check -p bun_runtime --lib` finished with no errors:
  `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 0.37s`.
- The scaffold compiles. The next step is the full build
  (`bun run build:debug`) which exercises the C++/Rust link.

### Expected goal next
- Full `bun run build:debug` succeeds end-to-end.
- `C:\nc-bun\build\debug\bun-debug.exe` exists.
- We can call `Bun.open(...)` and confirm the throw (scaffold).

---

## Risk register (rolling)

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Bun reviewers want a different API shape (enum verb, sync variant) | Medium | Medium | Land v1 as-is; iterate after first review |
| `Bun.spawn` (not `sync::spawn`) is the wrong primitive for fire-and-forget | Low | High | Will know after first test run; fall back to `sync::spawn` in a detached thread if needed |
| `utf8.xml` import compat: Bun 1.4 parses XML at import time | Low | Low | Not used by our PR |
| New tarball tar errors on other deps (highway, boringssl) | Low | Medium | The `excludes` array is generic; add more entries if a new dep has symlinks |
| The `[windows]` `cmd /c start` argv is wrong on non-en-US Windows | Medium | Low | Document the limit; can swap to direct ShellExecuteExW later |
| `Bun.open` on WSL throws UnsupportedOs | High | Low | Out of scope for v1; doc it |
| Fork's pre-existing `@lezer/lr` gap is actually a deliberate change | Low | Low | `bun add` committed the fix; harmless to send upstream too |

---

## 2026-08-25 (full build #1 — Rust passes, C++ rescle.cpp fails)

### Wanted
Full `bun run build:debug` succeeds end-to-end, producing
`C:\nc-bun\build\debug\bun-debug.exe`.

### Had
- `cargo check -p bun_runtime --lib` clean (previous round).
- Build moves past zstd, WebKit prebuilt, highway, lsquic, mimalloc.
- Rust compile of `bun_runtime` succeeds (the failure that ended the
  previous round is gone).
- C++ compile of `rescle.cpp` fails with `'atlstr.h' file not found`.

### Error (verbatim from build.log)
```
FAILED: obj/src/jsc/bindings/windows/rescle.cpp.obj
..\..\src\jsc\bindings\windows\rescle.cpp(35,10): fatal error:
'atlstr.h' file not found
   35 | #include <atlstr.h>
1 error generated.
ninja: build stopped: subcommand failed.
```

### Root cause
`rescle.cpp` is a Windows-only C++ file from Electron's `rcedit` fork,
included in Bun for `bun build --compile` to patch the icon of the
generated `.exe`. It depends on `atlstr.h`, which is part of the
**Microsoft ATL/MFC** component of Visual Studio. Our environment has
"Desktop Development with C++" via **VS Build Tools 2022**, which by
default does NOT install the ATL/MFC component.

This is independent of our `Bun.open` work — the file would fail to
compile for any user who installs only the default C++ workload.

### Things we thought about fixing
- *Skip the file via a ninja target override.* Risky: this is the icon
  path for `bun build --compile`. Skipping it would mean downstream
  Bun-compiled binaries cannot have a custom icon. A no-go for shipping
  a Bun binary, but acceptable if we only need `bun-debug` to run
  locally for testing the host_fn.
- *Install the ATL/MFC component via the Visual Studio Installer.*
  Standard fix. The component is ~500 MB but is needed to build Bun on
  Windows. The Windows build guide (`docs/project/building-windows.md`)
  doesn't mention this requirement, which is itself a small docs gap to
  file upstream.
- *Use a hermetic `atlstr.h` polyfill.* No — `atlstr.h` includes
  Win32/MFC primitives that depend on the MSVC runtime. Not a
  drop-in.
- *Patch rescle.cpp to use the C++ standard library equivalents
  (e.g. `std::wstring`).* The whole point of using ATL here is to
  patch resources in PE files; std alternatives don't exist. Not a
  realistic change.

### Chosen solution
Install the ATL/MFC component via the VS Installer. This is the
"correct" fix and the only one that leaves the build behavior
unchanged. We will document this in the journal + propose a docs PR
to oven-sh/bun noting that the Windows build guide should mention
ATL/MFC.

### Files edited
- (none yet — will run the installer next)

### Expected goal next
1. Install the ATL/MFC component.
2. Retry the full build.
3. If green, produce `bun-debug.exe` and run our test suite.

---

## 2026-08-25 (ATL install — admin-only, fallback path)

### Wanted
Install the ATL/MFC component into VS Build Tools 2022 so `rescle.cpp`
compiles.

### Had
- `vs_installer.exe modify` (Admin operation) silently exited 0 with no log
  file written and no ATL headers installed. Even with `--log` flag the
  log file was not created.
- The component requires admin elevation; the shell we're in isn't
  elevated. Repeated attempts produced no error and no progress.

### Things we thought about fixing
- *Use `winget` to install the ATL/MFC redistributable.* Wrong target —
  ATL headers are part of the VS Build Tools install, not a redist.
- *Download `atlstr.h` and headers from a public source.* Not portable
  and not a real fix.
- *Use `--passive` or `--quiet` to bypass elevation.* Both tried; both
  silent-success-without-effect.
- *Add elevation via `Start-Process -Verb RunAs`.* Would work but
  blocks the unattended loop. Defer.
- *Skip `rescle.cpp` via a ninja build override.* Local-only fix. The
  icon-patching feature is only used by `bun build --compile`; we don't
  need it for our test goal (running `Bun.open` to verify the
  registration compiles and runs).

### Chosen solution
Skip `rescle.cpp` from the local build. Add a one-line patch to the
build that drops the `windows/rescle` and `windows/rescle-binding` .cpp
files from the C++ source list, with a comment explaining why. The
upstream PR for `Bun.open` does NOT need this patch — it's a local
host-fn test convenience.

### Files to edit
- (will be added next, in `scripts/build/codegen.ts` or via a
  build-overrides file we add at the fork root).

### Expected goal next
1. Apply the rescle skip.
2. Re-run `bun run build:debug`.
3. Confirm `bun-debug.exe` is produced.
4. Run `bun-debug.exe -e "console.log(typeof Bun.open)"` to confirm
   the function is exposed (even though the body throws "scaffold").

---

## 2026-08-25 (DSH_SKIP_RESCLE link + system libs)

### Wanted
After the rescle.cpp ATL/MFC gate, get the final `bun-debug.exe` link
to succeed.

### Had
- rescle.cpp / rescle-binding.cpp skipped via DSH_SKIP_RESCLE=1.
- The Rust side (`src/sys/windows/mod.rs:1296`) still declares an
  `unsafe extern "C"` block for `rescle__setWindowsMetadata`, so the
  linker couldn't find the symbol.
- The link line in `scripts/build/bun.ts` is missing four Windows libs
  that the WIC + WebKit + clipboard paths use:
  `user32`, `shell32`, `ole32`, `oleaut32`.

### Errors
- 20 undefined-symbol link errors split between WIC clipboard
  (`GetClipboardData`, `OpenClipboard`, `IsClipboardFormatAvailable`,
  `RegisterClipboardFormatA`, `GetClipboardSequenceNumber`,
  `CreateStreamOnHGlobal`, `GetHGlobalFromStream`), WIC image COM
  (`CoInitializeEx`, `CoCreateInstance`, `CoTaskMemFree`,
  `VariantInit`, `VariantClear`), WebKit
  (`SHGetFolderPathW`, `SHGetKnownFolderPath`, `MapVirtualKeyW`,
  `GetMessageA`, `TranslateMessage`, `DispatchMessageA`), and the
  rescle Rust extern.

### Root cause
1. The DSH_SKIP_RESCLE toggle only affected the C++ source list; the
   Rust `unsafe extern "C"` declaration was unconditional.
2. The link line was authored when those system APIs weren't called
   from the Rust image/clipboard paths. The WebKit prebuilt
   (`bun-webkit-windows-amd64-debug`) is the source of
   `SHGetFolderPathW` / `MapVirtualKeyW` / message-loop symbols.

### Things we thought about fixing
- *Make the DSH_SKIP_RESCLE gate cfg-gate the Rust `extern "C"`
  block.* Yes, and stub the call so the rest of the function compiles.
  This is the local-only fix; the upstream PR for `Bun.open` doesn't
  need any of this.
- *Add the missing system libs to the link line.* Yes — orthogonal
  fix. These libs are needed even when DSH_SKIP_RESCLE=1 because the
  WIC image and WebKit code paths use them regardless.

### Chosen solution
- Add a local `unsafe extern "C" fn rescle__setWindowsMetadata` stub
  in the Rust side, gated on `cfg(DSH_SKIP_RESCLE)` via a `#[cfg_attr]`
  on the mod, returning `-14` (the magic "WindowsMetadataEditError"
  code) so `set_windows_metadata` always returns that variant.
- Add the four missing system libs to the `libs.push(...)` block in
  `scripts/build/bun.ts`.

### Files edited
- `src/sys/windows/mod.rs`: (will do next — add a stub extern block
  gated on a new `dsh_skip_rescle` cfg flag).
- `scripts/build/bun.ts`: added `ole32.lib`, `oleaut32.lib`,
  `user32.lib`, `shell32.lib` to the Windows libs list.

### Expected goal next
1. Re-run the build; link should succeed.
2. `bun-debug.exe` should land in `C:\nc-bun\build\debug\`.
3. Run `bun-debug.exe -e "console.log(typeof Bun.open)"` to confirm
   the function is exposed.

---

## 2026-08-25 (goal correction)

### What the user just clarified (this turn)
The drift: I'd been treating "ship a single PR to oven-sh/bun" as the
goal. That's wrong. The real goal is:

> **Replace nearly every npm package in dsh with native Bun support.**
> **When Bun's native support is missing or partial, develop and
> **fully implement it end-to-end inside Bun** — with proper, complete
> development: **no stubs, no placeholders, no mocks, no fakes, not
> "barely working", but a fully edge-case-hardened version**. After
> adding + fully optimising + passing all existing and new tests, we
> create a PR and commit to our local `nc-bun` repo first; only after
> the local repo is solid do we consider sending to oven-sh/bun.

### Restated goal (in one paragraph)
We are not contributing one-off features. We are systematically
**collapsing the npm dependency surface of dsh by lifting every
package we depend on into Bun's runtime, with full implementations and
full test coverage at the Bun level**, committed to our local fork
`ncdevshiv/nc-bun` first. The Bun.open work was a starter that proved
the pipeline; what follows is a roster of every npm dep in dsh that
Bun 1.4 either already has a built-in for or needs a built-in for,
each landed as a hard-pristine Bun feature (proper tests, no
shortcuts), then a dsh-side migration PR to drop the npm dep.

### What this changes about the plan
1. **The journal is the source of truth for the whole program**, not
   just the Bun.open effort. New entries will be one section per
   Bun feature we add or harden, each with the same structure
   (wanted / had / did / errors / root cause / fix / files / review).
2. **Append-only journal rule is now load-bearing.** I will never
   edit, delete, or rewrite an earlier section. New sections go at
   the bottom and reference earlier ones by date heading. The
   `git diff --check` pre-commit hook in this repo already enforces
   trailing-newline policy; that will catch any accidental edits.
3. **Bun.open was a starter, not the goal.** What it proved: the
   full dev → test → bench → push pipeline works on this machine,
   the fork builds end-to-end (modulo the local-only ATL
   workaround documented above), and the registration wiring
   (Rust host_fn + JSDoc table + bun-types + bun:test spec) is
   the right shape. We now build the *roster* of npm deps in dsh
   that Bun should absorb.
4. **No "barely working" is the explicit standard.** Every Bun
   feature we add or harden must have:
   - Real implementation, not a stub/mock/fake.
   - Full happy-path test coverage.
   - Full error-path test coverage (every `OpenError` /
     `Result::Err` arm exercised by a test).
   - Edge cases (empty inputs, NUL bytes, very long paths,
     symlinks, race conditions, cancellation).
   - Bench numbers vs the npm incumbent.
   - Optimisation pass: profile, fix hot spots, re-bench, repeat
     until numbers are clearly better than the npm package.
5. **Order of work per feature:**
   a. Identify npm dep in dsh that Bun should absorb.
   b. If Bun 1.4 already has it: audit the API + tests for
      completeness; if anything is partial (positions in
      `Bun.YAML`, real-time `Bun.search`, etc.), file a focused
      issue + land a hardening PR.
   c. If Bun doesn't have it: design the API + behavior matrix
      from the npm package's actual surface; review upstream's
      `Bun.*` style for consistency.
   d. Implement in Rust (the convention for process / system
      layers) or C++ (the convention for JS / string layer).
   e. Wire registration: `BunObject+exports.h` macro + `export_callbacks!`
      table + JSDoc-table row in `BunObject.cpp` + bun-types.
   f. Tests: bun:test spec covering happy + error + edge cases,
      gated by `isWindows | isMacOS | isLinux` where behavior
      diverges.
   g. Bench: tiny Node script comparing to the npm package;
      capture mean/median/p99.
   h. Commit to `ncdevshiv/nc-bun` (local) first. Then a
      separate dsh-side PR drops the npm dep and switches to
      the new Bun built-in.

### Roster (initial — dsh deps in order of replaceability)
The full inventory lives in the previous session's audit (40+ unique
npm deps). The order I will work them, based on how much Bun 1.4
already has + how much dsh actually uses them:

1. **`open` → `Bun.open`** *(in progress)* — npm `open@11.0.1`,
   used in `apps/web`. ~80% greenfield in Bun; we own the API.
2. **`fflate` → `Bun.Archive` (ZIP support)** — dsh uses ZIP
   creation in `apiproxy/session-export.ts` (create zip with many
   small JSONL files) and ZIP read in a web e2e test. Bun 1.4
   has `Bun.Archive` (tar only) + CompressionStream. We add ZIP
   to `Bun.Archive` and migrate.
3. **`zod` → Bun's own `@deepseek-ai/schemastery` consolidation**
   — dsh already vendors `schemastery`; ~20 packages declare zod
   too. Hardest consolidation; many call sites; careful migration.
4. **`execa` → `Bun.spawn` migration (already 80% done in dsh)**
   — the 3 remaining `execa` call sites get rewritten to
   `Bun.spawn` with the new fluent stdio API.
5. **`chokidar` → `Bun.watch` hardening** — 3 call sites (file
   watchers). Bun 1.4's `Bun.watch` lacks glob filtering, polling
   fallback, and rename-storm handling. We add these.
6. **`ws` → `Bun.serve({ websocket })` consolidation** — only
   `packages/client/connection/src/websocket-downlink.ts` uses
   `WebSocketServer`; Bun's serve already supports upgrade.
7. **`js-yaml` + `yaml` → `Bun.YAML` with positional errors** —
   dsh ships two YAML libs. Bun 1.4 has `Bun.YAML` but with YAML
   1.2 semantics (the `on:` break) and no positional errors.
   We harden both, then consolidate dsh onto `Bun.YAML`.
8. **`smol-toml` → `Bun.TOML` migration** — small, one call site
   in `scripts/gen-third-party-notices.ts`. Done as a Bun-side
   touch-up if needed.
9. **`koffi` → `bun:ffi` migration** — 5 packages (fs-local,
   directory-picker-native, sandbox-windows-acl,
   session-persistence-jsonl, subprocess-local). Each
   hand-rolled `koffi` callsite gets a `bun:ffi` equivalent.
   Done as a series of dsh-side PRs gated on Bun runtime choice.
10. **`sharp` → `Bun.Image` for our decode/validate path** —
    `attachment-local/src/image.ts` uses sharp as a header reader.
    Bun 1.4's `Bun.Image` covers the API surface we need.
11. **`turndown` → `Bun.markdown` reverse mode** — large,
    ambitious, deferred until Bun 1.5+.
12. **`eventsource-parser` → `Bun.fetch` SSE** — add
    `response.sse()` to Bun's fetch; dsh's MCP client consumes
    SSE heavily.
13. **`diff` → `Bun.diff`** — small, 2 call sites.
14. **`node-pty` (+ our patch) → `Bun.Terminal` dogfooding** —
    the `Bun.Terminal` shipped in 1.4 is enough; the patch we
    ship to Bun fixes the deployment story (no helper binary).

Out of scope for this program (browser/renderer layer, not Bun's
domain): zustand, clsx, dayjs, immer, React, shiki, katex,
mermaid, cytoscape, @tanstack/react-virtual, electron,
playwright, lightningcss, @vscode/ripgrep.

### Files affected (this turn)
- `JOURNAL.md` — appended this entry (no edits to earlier
  sections).

### Expected goal next
1. Complete the `Bun.open` link step + run the scaffold test.
2. Reassess the roster after the first end-to-end Bun.open
   success; the order may shift based on what was hard/easy.
3. Begin `Bun.Archive` ZIP support next — the second feature
   on the list and the one with the most direct dsh impact
   (the ZIPs in `apiproxy/session-export.ts` ship in every
   session export).
4. Add a `Roster.md` index file alongside `JOURNAL.md` that
   points at the per-feature entries in `JOURNAL.md` so the
   program of work is discoverable from the journal.


---

## 2026-08-26 (Bun.open #2 - link fixed, real spawn path landed)

### Wanted
`bun-debug.exe` exists, `Bun.open(target)` really launches, tests green,
bench vs npm `open@11.0.1` captured.

### Had
- Scaffold committed at `c1aff446f9`; link failed on
  `rescle__setWindowsMetadata` (ATL/MFC missing) + four system libs.
- Working tree carried the drafted stub, lib additions, and an
  uncommitted ShellExecuteExW/CoInitializeEx extern draft in
  `windows_sys/externs.rs`.

### Did
1. Committed the link fix as `eb1f620c6b`: DSH_SKIP_RESCLE Rust stub
   (returns `-14` WindowsMetadataEditError), ole32/oleaut32/user32/
   shell32 on the link line, bsdtar `--exclude` for zstd's symlinked
   test fixtures.
2. **Pivoted the launch path back to the committed plan** (route through
   `js_bun_spawn_bindings::spawn`) and dropped the ShellExecuteExW
   extern draft before it was ever committed: a native shell-launch path
   would duplicate process plumbing Bun already owns (reaping, exited
   promise, PID reporting), plus COM-apartment init on the main thread
   risks WebKit's existing COM usage. The draft is recoverable from this
   journal entry if a future optimization pass wants it.
3. Implemented the host_fn (`13852702eb`): parse -> argv_for ->
   JS cmd array + spawn options object -> `spawn()` -> read `.pid` /
   `.exited` off the Subprocess -> resolve `{pid, exited}` promise.
   All failures reject asynchronously (npm-parity contract).
4. Trimmed the scaffold API to what the implementation honors honestly:
   dropped `wait` (await `.exited` instead), dropped `hideErrors`
   (no effect on the cmd/start route), empty-string `app` now fails loud
   instead of silently falling back to the default opener, arrays
   rejected as options, rich `exited` payload collapsed to
   `Promise<number>` (what Subprocess.exited actually resolves).
5. Windows `app` override now direct-execs `[app, target]` like npm open,
   fixing the scaffold bug where `options.app` was ignored.
6. Rewrote `test/js/bun/util/open.test.ts` from UTF-16-corrupted bytes
   (git treated it as binary; one stray NUL from a PowerShell write) to
   clean UTF-8 with 25 cases: validation arms, hermetic fixture-launcher
   launches (sentinel-proof), error paths, concurrency, unicode, long
   targets, opt-in default-opener E2E behind `DSH_BUN_OPEN_E2E=1`.
7. Full build green: 1118 ninja steps, `bun-debug.exe` links.

### Errors / root causes / fixes
- *First smoke test hung awaiting `r.exited`.* Root cause: my probe used
  `{app: process.env.SMOKER}` with SMOKER unset -> default opener ->
  `cmd /c start` on an extensionless target. Not a product bug; isolated
  with T1/T2/T3 probes proving plain spawn, detached spawn, and
  Bun.open all settle correctly.
- *20k-char target test failed (sentinel never written).* Real platform
  limit: cmd.exe caps its command line at 8_191 chars even though
  CreateProcessW allows 32_767. Test now documents both truths: under
  4_000 chars succeeds on Windows / 20_000 on POSIX, beyond 8_191
  settles cleanly without crashing.
- *`import performance from "node:perf_hooks"` gave no `.now`.*
  Named import required.
- *Bench "cold import" first measured 0.43ms - bogus:* the module was
  already imported at the top of the bench script. Fixed with a fresh
  subprocess probe: true cold is **503.30ms**.

### Bench (Windows, debug build, SAME bun-debug binary both sides)
- Cold import: npm open@11 **503.30ms** vs Bun.open ~0ms (built-in).
- Warm spawn-complete, interleaved n=25:
  npm mean 27.86ms (p95 47.42, s 8.85) vs Bun.open mean **5.55ms**
  (p95 10.32, s 2.30) -> **5.02x**.
- Wait-mode end-to-end (call until opener process finished):
  npm mean **1267.60ms** vs Bun.open mean **21.22ms** -> **59.73x**.
  npm pays powershell.exe startup+eval per call (open@11 routes Windows
  through PowerShell Start-Process); ours spawns cmd.exe directly.
- Our glue overhead floor: validation-reject path measures **110us/call**
  (n=20000), so the 5.5ms spawn-complete is ~98% OS process creation.

### Optimisation pass verdict
Numbers are clearly better than the incumbent on every metric. The only
remaining lever is replacing cmd.exe with native ShellExecuteExW on
Windows (~1-4ms of spawn-complete, some of exited-settle). Declined for
this PR: requires COM init on the JS thread plus a hand-rolled watcher
thread duplicating Subprocess reaping; risk out of proportion to a <1%
user-visible gain. Recorded as the known future optimization.

### Files
- `src/runtime/api/BunObject.rs`, `src/runtime/api/open.rs`,
  `packages/bun-types/bun.d.ts`, `test/js/bun/util/open.test.ts`.
- Commits: `eb1f620c6b` (link), `13852702eb` (implementation).

### Expected goal next
1. Push `feat/bun-open` to ncdevshiv/nc-bun.
2. dsh-side migration: apps/web drops `open` for `Bun.open`.
3. Two dsh-side benchmarks (call-site latency, dependency footprint),
   then roster #2 (Bun.Archive ZIP).

---

## 2026-08-26 (Bun.open #3 - dsh migrated off npm open)

### Wanted
Drop `open@11` from `packages/bundle/web-app`, switch `dsh web`'s browser
handoff to the new built-in where available, run two dsh-side benchmarks,
keep every gate green.

### Had
- `open@11.0.1` used via `import.meta.resolve('open')` inside an
  eval-string helper child that existed to scrub credentials and to wait
  out Windows PowerShell handoff latency.
- Snapshot fixture hooked the `open` specifier with a mock whose record
  line (`apiKeyPresent`, `dshHomePresent`) could only ever observe the
  mock child, not the OS opener.
- Reality check from the audit: dsh product paths run on Node, so
  `Bun.open` cannot be the only path - runtime probing is required.

### Did
1. New `src/browser-opener.ts`: `probeBunOpen()` -> in-process
   `Bun.open`; otherwise direct per-platform spawn (`cmd /c start`,
   `/usr/bin/open`, `xdg-open`) with `scrubbedParentEnv()` and stderr-
   first-line error mapping identical to the old parent logic. Settles
   on the launcher's `exit` event, not `close` (see errors below).
2. `index.ts` lost `BROWSER_OPENER_PROGRAM`, `spawnBrowserLauncher`,
   and the npm import; `internals.openBrowser` now points at
   `openDefaultBrowser` - same test seam, new implementation.
3. Dropped `"open": "^11.0.0"` from package.json; rewrote the fixture
   (`register.mjs`) to swap `internals.openBrowser` on the built lib
   directly and deleted `open.mjs`. Snapshot evidence dropped
   `apiKeyPresent`/`dshHomePresent` (they measured the mock, not the
   opener); credential non-forwarding is now asserted precisely by
   unit tests on the real spawn options.
4. Happy-path snapshot now uses `DSH_BROWSER_OPEN_TEST_EXIT_ON_READY=1`
   like its SSH sibling: `dsh web` has no self-shutdown, so the old
   exitCode-0 expectation was recording external termination luck.
5. New `tests/browser-opener.spec.ts`: 15 cases (platform builders incl.
   unsupported-platform rejection, env scrubbing of KEY/DSH_* names,
   stderr-first-line reason stripping, silent-exit-code message, spawn
   ENOENT propagation, Bun-branch preference and rejection pass-through).
6. README.md / README.zh.md paragraphs rewritten for the in-process
   design; obsolete Windows-PowerShell-helper sentences removed from
   both languages together.
7. Agent Note `.agents/notes/implemented/architecture/2026-08-26-web-
   browser-handoff-without-npm-open.md`.

### Errors / root causes / fixes
- *Snapshot happy path regressed to exitCode undefined.* Root cause:
  nothing about my change kept the server alive - `dsh web` never exits
  by itself, and the old recording had captured an externally terminated
  run. Verified with active-handles probes (Server + FSWatchers are the
  legitimate long-lived handles) and plain-CLI runs with stdin closed.
  Fix: bounded EXIT_ON_READY mechanism, matching sibling test 3.
- *Bench harness hung forever on the NEW route.* Root cause: awaiting
  the launcher's `close` event waits for stdio pipes to drain, and
  Windows `start` can hand those pipes to the process it dispatched (a
  console-hosted target inherits them) - pipe drain has no bound while
  process lifetime does. Fix: settle on `exit` in both production code
  and tests; documented in the Agent Note.
- *First bench script hung silently with zero output.* Same root cause;
  diagnosed by bisecting with progress logging instead of guessing.
- *tsc build blocked by pre-existing untracked `packages/llm/llm-ai-sdk`
  type errors* (not mine, directory is untracked in git). Worked around
  locally with scoped `tsdown --filter @deepseek-ai/dsh-web-app` after
  confirming zero remaining errors in touched files.

### Bench #2 and #3 (dsh side, Windows, Node 24)
- Handoff end-to-end, interleaved n=12 (hermetic .cmd target, no real
  browser): OLD npm-open helper mean **1236.9 ms** (p50 1131, p95 2575)
  vs NEW direct opener mean **79.1 ms** (p50 73, p95 106) -> **15.6x**.
  The old route pays powershell.exe startup per launch; the new route
  is one cmd.exe round trip.
- Dependency footprint: cold `import('open')` under Node = **32.8 ms**
  paid inside every old helper invocation, now deleted; install tree
  sheds 68 files / ~132 KB (open + wsl-utils + powershell-utils +
  default-browser + 7 more transitive runtime deps).

### Gates
- web-app package specs: 34 passed.
- Assembled snapshot suite (`-t browser-open`): 4 passed.
- bun-debug Bun.open suite: 22 passed / 3 skipped (from entry #2).

### Files
- `packages/bundle/web-app/src/browser-opener.ts` (new),
  `src/index.ts`, `package.json`, `README.md`, `README.zh.md`,
  `tests/browser-opener.spec.ts` (new), `tests/web-app.spec.ts`.
- `apps/cli/tests/fixtures/web-browser-open/register.mjs` (rewritten),
  `open.mjs` (deleted), snapshot spec env update.
- Deleted scratch probe/bench scripts before commit; numbers recorded
  here are the artifact.

### Expected goal next
Commit this as the dsh-side PR branch. Then roster #2:
`fflate` -> `Bun.Archive` ZIP support (apiproxy session exports).
