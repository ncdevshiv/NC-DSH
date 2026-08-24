# Agent Note: The plugin inventory replaces a running HMR duplicate instead of failing the toggle

Status: implemented

English | [中文](2026-08-24-plugin-inventory-hmr-duplicate-replacement.zh.md)

## Problem

On every `dsh web` host, enabling the HMR row from the Web settings UI always failed:

```
Toggle failed: failed to apply loader entry hmr (@deepseek-ai/cordis-plugin-hmr): service "hmr" has been registered at <Hmr>
```

Two composition facts produced an unavoidable collision. The `dsh-web-app` bundle disables the shared module-reload `hmr` row (`# TODO: Re-enable shared HMR for Web after its reload lifecycle is tested.`), and `profile-boot` then mounts a watch-only HMR fallback (`root: []`) through `loader.create`, because the documented patch-layer hot-reload contract needs an `hmr` service even when the row is disabled. The settings card for the disabled row therefore promised an enable that could never work: mounting it starts a second instance in the same realm, Cordis rejects the duplicate service registration fail-loud, the loader rolls the entry back to disabled, and the gateway surfaced that raw chain.

## Decision

Two changes make the toggle honest about what actually runs:

- **The gateway replaces instead of failing** (`packages/host/plugin-inventory`). After a real failed enable, `setEnabled` collects active same-module entries — same `options.name`, running fiber, not group rows — disables them, retries the enable once, and restores every displaced entry when the replacement itself fails. Displacement triggers only after a genuine collision, so entries providing distinct isolated instances are never disturbed; an unrelated failure with no duplicates reaches the caller verbatim.
- **Patch watches follow the `hmr` service** (`@deepseek-ai/dsh-app-boot`). The boot-time registrations moved from two direct `watchUserPatches` calls into `watchUserPatchesAcrossHmrSwaps`, which registers them inside a `user-patch-watch` plugin requiring `hmr`. Cordis unloads those registrations together with each instance they were registered on and re-runs them against the next instance that mounts, so replacing the fallback keeps `cordis.patch.yml` hot-reload alive without a restart. The first application still settles at boot and fails loud through the existing suppression path; later applications surface failures through their fiber state.

## Alternatives considered

**Reject enables with a message naming the running duplicate.** Honest but useless: the toggle stays broken unless the user hand-performs the swap, and the manual order (disable the fallback first) silently kills patch watching — the exact loss the second change fixes. A clearer error would have dressed up a dead end.

**Let profile-boot reuse the config row instead of creating a fallback entry** (enable `include:hmr` live with overridden config when no service exists). It hijacks a bundle-disabled row's meaning, puts full module-reload HMR on Web against the explicit TODO, and still breaks when the user toggles the row off.

**Parse the duplicate-registration error text to decide when to displace.** Fragile coupling to a vendor diagnostic string; the behavioral trigger (enable failed while a same-module instance runs) is observable without parsing and self-corrects — if displacement was not the cure, the retry fails and everything is restored.

## Consequences

Enabling the disabled HMR row on Web now works: the full module-reload instance replaces the watch-only fallback for this process, and the displaced entry stays disabled until another toggle or a restart (the web-app TODO's caution about the untested reload lifecycle now belongs to whoever flips the switch). The reverse trap remains visible by design: disabling the last HMR instance stops patch watching until some instance mounts again or the process restarts; nothing pretends otherwise.

A transient blip is possible when an enable fails for an unrelated reason while a same-module sibling runs: the sibling is displaced and restored around the doomed retry. Restoration normally succeeds because the displaced module was running moments earlier; when it does not, the failure names each unrestored entry.

## Testing

`packages/host/plugin-inventory/tests/inventory.spec.ts` pins replacement success, restoration after a synthetic retry failure, and the verbatim unrelated failure. `packages/boot/app-boot/tests/user-patches.spec.ts` swaps two loader-entry HMR instances under a live watcher and requires edits to land through the second instance, plus the missing-include fail-loud path.
