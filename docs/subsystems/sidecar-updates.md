# Sidecar Updates

English | [中文](sidecar-updates.zh.md)

The auto-update pipeline for the [`ai-sidecar`](../../packages/sdk/sidecar-updates/README.md) model-engine binary. [`ctx.sidecarUpdates`](../../packages/sdk/sidecar-updates/README.md) polls a GitHub repository's latest release, verifies the platform asset against the release's `SHA256SUMS` manifest, stages and installs it into a versioned directory layout, and atomically repoints a `current.json` pointer. Every tag installs into its own directory and every document replaces atomically, so installing never overwrites a running executable and a crash leaves either the complete previous or the complete next pointer. Check and install outcomes surface through the [`notifications` seam](notifications.md) and the `sidecar-updates/status` event; nothing here touches a model request, and the new binary is picked up when the model engine next starts.

Source: [`packages/sdk/sidecar-updates/src/index.ts`](../../packages/sdk/sidecar-updates/src/index.ts)

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `bun run verify-cordis-catalog` in doc-sync; regenerate with `bun run gen-cordis-catalog`) — this section is byte-identical in both language sides of the page. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

<a id="ctxsidecarupdates--sidecarupdatesservice"></a>

### `ctx.sidecarUpdates` — `SidecarUpdatesService`

GitHub-release update pipeline reporting through the notification seam.

```ts cordis-catalog
/**
 * Cached sync view of the pipeline state. The pointer and ignore documents
 * are re-read per call, so externally changed files are reflected without
 * cache invalidation.
 * @returns a frozen status snapshot.
 */
status(): UpdateStatus

/**
 * Run one release check now: fetch `releases/latest`, refresh the cached
 * comparison state, reconcile the actionable notification, and — on the
 * first successful check with nothing installed and auto-install enabled —
 * install that release. Transport, HTTP, and parse failures set
 * `lastError`, warn, and never throw.
 * @returns the committed post-check status.
 */
async checkNow(): Promise<UpdateStatus>

/**
 * Download, checksum-verify, and install one release, then atomically
 * repoint the pointer document. Only the published latest release can be
 * installed; the bytes stage under `downloads/<tag>/` and land under
 * `releases/<tag>/`, so a running binary is never overwritten.
 * @param requestedTag - tag to install; defaults to the latest published
 *   release. Any other tag fails with `UNKNOWN_RELEASE`.
 * @returns the committed pointer entry plus `restartRequired`.
 * @throws SidecarUpdateError on lookup, unsupported target, missing asset
 * or checksum manifest, download, or digest mismatch failure.
 */
async install(requestedTag?: string): Promise<InstallResult>

/**
 * Add a release tag to the persisted ignore list, suppressing its
 * "update available" notification until it is removed from
 * `ignored.json` (or the seed in settings). Ignoring an already-ignored
 * tag changes nothing. Persistence is synchronous; the promise confirms
 * the committed state.
 * @param tag - exact release tag to ignore.
 * @returns a promise settling after the list and status are committed.
 */
ignore(tag: string): Promise<void>
```

Source: [`packages/sdk/sidecar-updates/src/index.ts:192`](../../packages/sdk/sidecar-updates/src/index.ts)

<a id="sidecar-updates-events"></a>

### `sidecar-updates/*` events

<a id="sidecar-updatesstatus--emit"></a>

#### `sidecar-updates/status` — emit

Complete pipeline status after every committed check or mutation: check completion, install, and ignore each emit one snapshot.

```ts cordis-catalog
/**
 * Complete pipeline status after every committed check or mutation:
 * check completion, install, and ignore each emit one snapshot.
 * @param status - the frozen full-status payload.
 * @mode emit
 */
'sidecar-updates/status'(this: SidecarUpdatesService, status: UpdateStatus): void
```

Source: [`packages/sdk/sidecar-updates/src/index.ts:187`](../../packages/sdk/sidecar-updates/src/index.ts)
<!-- END GENERATED cordis-surface -->
