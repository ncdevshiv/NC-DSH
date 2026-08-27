# @deepseek-ai/dsh-sidecar-updates

English | [中文](README.zh.md)

The auto-update pipeline for the `ai-sidecar` model-engine binary. The service polls a GitHub repository's latest release, verifies the platform asset against the release's `SHA256SUMS` manifest, stages and installs it into a versioned directory layout, and atomically repoints a `current.json` pointer. Check and install outcomes surface through the notification seam ([`ctx.notifications`](../../host/notifications/README.md)) and the `sidecar-updates/status` event; nothing here touches a model request — the new binary is picked up when the model engine next starts.

## Layout

```text
<installDir>/current.json                     pointer: {tag, asset, sha256, installedAt, exePath}
<installDir>/ignored.json                     tags ignored through ignore()
<installDir>/downloads/<tag>/<asset>          staged download bytes
<installDir>/releases/<tag>/ai-sidecar[.exe]  installed executables
```

Every tag installs into its own directory and every document replaces atomically (random-suffix exclusive-create temp plus rename), so installing never overwrites a running executable and a crash leaves either the complete previous or the complete next pointer. A corrupt pointer or ignore list reads as absent rather than blocking installs.

## Configuration

| key | default | meaning |
|---|---|---|
| `repo` | `ncdevshiv/ai-sdk` | `owner/name` repository polled for releases. |
| `installDir` | `<cwd>/core-deps/ai-sdk` | Install directory root holding the pointer, releases, and downloads. |
| `checkOnStart` | `true` | Run one check after startup. |
| `intervalMs` | — | Poll interval in milliseconds inside [60s, 24h]; omit to disable polling. |
| `assetPrefix` | `ai-sidecar` | Asset names publish as `{prefix}-{platform}-{arch}[.exe]`. |
| `autoInstallOnFirstRun` | `true` | Install the first observed release when nothing is installed yet. |
| `apiBase` | `https://api.github.com` | GitHub API base URL (override points tests and mirrors elsewhere). |
| `ignoredVersions` | `[]` | Read-only seed of the ignore list; grown at runtime via `ignore()`. |

The optional `sidecar-updates` settings namespace layers user overrides over the composition entry config through [`dsh-settings`](../../settings/settings/README.md). The section's `ignoredVersions` field seeds the ignore list; the authoritative runtime list is `<installDir>/ignored.json`, which `ignore()` extends.

## Service

`status()` returns a frozen snapshot `{ installed, latest, updateAvailable, ignoredLatest, lastError? }` re-read from the live documents on every call. `updateAvailable` means the newest release is not ignored and no newer-or-equal release is installed; `lastError` describes the most recent failure.

`checkNow()` fetches `releases/latest`, refreshes the cached comparison state, reconciles notifications, and — on the first successful check with nothing installed and `autoInstallOnFirstRun` — installs that release. Transport, HTTP, and parse failures set `lastError`, warn, and return the status instead of throwing.

`install(requestedTag?)` downloads the target asset and its `SHA256SUMS`, verifies the digest, stages under `downloads/<tag>/`, writes `releases/<tag>/ai-sidecar[.exe]`, repoints `current.json`, publishes an installed notice, and emits status. Only the published latest release can be installed; any other tag fails with `UNKNOWN_RELEASE`. All failures throw a typed [`SidecarUpdateError`](./src/github.ts) carrying a stable machine code (`UNSUPPORTED_PLATFORM`, `ASSET_MISSING`, `CHECKSUM_MANIFEST_MISSING`, `CHECKSUM_ENTRY_MISSING`, `CHECKSUM_MISMATCH`, `DOWNLOAD_FAILED`, ...).

`ignore(tag)` appends to the persisted ignore list and reconciles notifications immediately: ignoring the newest release clears `sdk-update:*` notices and sets `ignoredLatest`.

## Notifications

An actionable check publishes id `sdk-update:{tag}` (kind `sdk-update`) titled "AI SDK update available" with body `Installed {installed} → available {tag}`. The entry refreshes only when its content changes, so dismissing one stays dismissed until a different release or install state makes the content stale; stale `sdk-update:*` entries are deleted. A completed install publishes id `sdk-update-installed:{tag}` (kind `sdk-update-installed`) titled "AI SDK {tag} installed".

## Model Experience

### Sidecar update infrastructure

#### What the model sees

Nothing. The pipeline registers no tool, prompt section, or Session event; it manages files under `installDir` and notifications in the Host seam, and the updated binary only affects which engine serves the next process lifetime.

#### Token effect

Zero. Release metadata, digests, download bytes, and status snapshots never enter a model request; even the `sdk-update:*` notices live outside it.

#### KV Cache effect

Independent. Checks, installs, and ignores do not touch any model request prefix; swapping the sidecar binary between sessions cannot invalidate an otherwise reusable provider cache entry.

## Known Limitations and Deferred Work

- **Windows locked executables** — a running `.exe` cannot be replaced, so installs always write a fresh versioned directory and repoint the pointer; old release directories are never garbage-collected and removal stays manual.
- **Unauthenticated GitHub access** — release lookups send no token and inherit the anonymous limit of roughly 60 requests per hour per address; polling intervals below minutes will exhaust it.
- **Checksum trust model** — integrity is verified against the release's own `SHA256SUMS` fetched over HTTPS from the same release; signatures, provenance, or pinning beyond the digest are out of scope, so a compromised release compromises the install.
- **Only the latest release installs** — `releases/latest` is the single lookup, so pinning, downgrade, or arbitrary-tag installation requires a different discovery contract.
- **No restart orchestration** — the pipeline reports `restartRequired` but never restarts or drains the model engine itself.
