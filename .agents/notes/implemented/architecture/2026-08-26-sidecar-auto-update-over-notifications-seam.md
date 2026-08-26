# Agent Note: Sidecar auto-update pipeline over a shared notifications seam

English | [中文](2026-08-26-sidecar-auto-update-over-notifications-seam.zh.md)

Status: implemented

## Problem

The `ai-sidecar` model-engine binary ships outside npm and had no update path: an operator had to watch releases, download the right platform asset, trust it, replace the binary, and restart by hand. Any future host or client surface that wanted to surface update state would need its own store, dismissal handling, and event plumbing, duplicating that seam per feature.

## Decision

Two plugins own the capability end to end. `@deepseek-ai/dsh-notifications` is a generic dismissible-notification Service Definition under `ctx.notifications`: stable-id publish/replace semantics, read/dismiss/delete state, one atomically rewritten JSON document under `<harness home>/notifications/v1/state.json`, contained `notifications/updated`/`notifications/removed` events, and an invariant companion asserting the event streams against the store. `@deepseek-ai/dsh-sidecar-updates` is the updater consumer: it polls GitHub's `releases/latest`, verifies the `{prefix}-{platform}-{arch}` asset against the release's `SHA256SUMS`, installs into a per-tag versioned layout, and repoints `current.json` atomically, so a running executable is never overwritten. Every check, install, and ignore commits a status snapshot through the notifications seam (`sdk-update:{tag}`, `sdk-update-installed:{tag}`) and the `sidecar-updates/status` event. Ignored versions live in `<installDir>/ignored.json`; the settings section's `ignoredVersions` field is only the seed merged at load. The optional `sidecar-updates` settings namespace layers user overrides over the composition entry config through `installSettingsSection`.

## Alternatives considered

- **Updater writes settings itself** — mutating other namespaces from inside the updater couples it to the settings write path for one array field. Lost to an install-directory file: self-contained, atomic with the rest of the pipeline state, and still seeded by config.
- **In-place binary replacement** — overwriting `ai-sidecar` breaks on Windows (a running `.exe` is locked) and turns any crash mid-swap into a broken engine. Lost to the versioned-directory-plus-pointer layout, which makes installs all-or-nothing at one rename.
- **Notifications as a sidecar-updates private store** — a private table would work today but forces every future notifier (jobs, workflows) to duplicate persistence and dismissal. Lost to a shared seam whose producers stay storage-free.
- **Full semver engine** — tags here are numeric-dot release names; a dependency would add surface without current evidence. Lost to a strict numeric-dot comparator where missing segments count as zero.

## Consequences

Update state survives crashes at every step: staged downloads, per-tag executables, and the pointer each commit atomically, and the invariant companion fails any status whose pointer names a missing executable. The trade-offs accepted: integrity stops at the digest of a release's own manifest (no signature verification), unauthenticated GitHub lookups inherit the anonymous rate limit, only the published latest release can install, and removal of superseded release directories remains manual.
