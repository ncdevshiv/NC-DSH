# Agent Note: landlock-run rewritten from C11 to single-file Rust

Status: implemented

English | [中文](2026-08-23-landlock-run-rust-port.zh.md)

## Problem

The launcher was the one repository component where memory safety rested entirely on review discipline: it is the first process untrusted command lines reach, it sets `no_new_privs` to neutralize setuid/setgid escalation inside sandboxes, and its failure mode is silent policy violation rather than a crash. C offers no backstop there — a bounds mistake in grant parsing would ship as a confinement bug. The toolchain also capped the native platform story: the Windows tree-termination gap (best-effort `taskkill /T /F`) closes only with kernel-guaranteed Job Object semantics, and a second hand-audited C codebase would make that rung harder to keep trusted.

## Decision

The launcher is a single-file Rust crate at `native/landlock-run/packages/entry/native/` (`Cargo.toml`, `src/main.rs`, `Cargo.lock`). Its only dependency is the `libc` crate's syscall shims; the Landlock UAPI structs and access bits remain self-defined verbatim in the source file, preserving the audit-record rule. The release profile pins `panic = "abort"`, so any future panic still fails closed — the process dies without exec'ing the wrapped command.

The CLI contract ([cli-contract](../../../../native/landlock-run/docs/cli-contract.md)) is unchanged byte for byte: argv grammar, exit `125` on every launcher failure, the probe report lines, and the partial-enforcement notice. `test/launcher.test.js` proves those strings against the real binary and passes unchanged. The entry package ships the audited source in its tarball as before, now under `native/` together with `Cargo.lock`.

Builds stay native-only per architecture. `scripts/build.ts` runs cargo against Rust's bundled static musl target: `rustup target add <triple>` installs only the standard library, rustc ships self-contained CRT objects for the musl targets, and the host C compiler merely drives linking — no `musl-gcc`. Binaries still land in `packages/<name>/bin/landlock-run`, so `verify-launcher-binary.mjs` and the pack rehearsal pass without modification. CI's builder-of-record jobs add the rustup targets beside the apt step.

## Testing

- `test/launcher.test.js` — usage errors, exit-code passthrough, denied-write world-proofs, inheritance across exec, fail-closed on unopenable grants — passes unchanged on the Linux CI legs under `NALR_REQUIRE_LANDLOCK=1`.
- Entry tests, the ELF prepack gate, and the packed-install rehearsal exercise identical artifacts and contracts; `cargo check` and `cargo clippy` run clean for both musl triples.

## Alternatives considered

**Keep the C11 source.** Rejected: the code was already audited and the contract stable, but every hardening or platform change re-opens manual memory-safety review, and the language contributes nothing to the security argument that the compiler-enforced guarantees now provide.

**Adopt the community `landlock` crates.io crate.** Rejected: it moves the UAPI definitions out of the audited file into transitive dependency versions, trading the self-defining audit record for zero functional gain — the launcher needs three syscalls and two structs.

**`no_std` Rust.** Rejected: argv handling, allocation, and `execvp` want std anyway; `panic = "abort"` plus the one-crate locked dependency set already bounds the runtime surface, and dropping std would only add unsafe plumbing.

**Rewrite in Zig.** Rejected: same safety motivation with a younger toolchain; static musl output is a first-class Rust target today and nothing in the launcher needs Zig-specific capabilities.

## Consequences

- Memory safety at the confinement boundary no longer depends on review alone, and the audit surface remains one reviewed source file plus one pinned dependency.
- The trust base gains rustc, std, and `libc` for the build, pinned through `Cargo.lock` and the CI builder images; consumers still receive only prebuilt static binaries, so the added surface is build-time, never distribution-time.
- Allocation failure aborts through the Rust allocator instead of printing `landlock-run: out of memory`; both paths die before exec, and the contract pins only the fatal-prefix format, which is preserved.
- Building the launcher requires cargo plus the musl std targets instead of `musl-tools`; the workspace commands documentation carries the prerequisite.
- Future native confinement work (for example the planned AppContainer/Job-Object Windows runner) inherits a template whose security-critical core is written in a language whose compiler enforces the invariant.
